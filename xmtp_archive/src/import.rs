fn insert<Client>(
    element: BackupElement,
    client: &Client,
    provider: impl MlsProviderExt,
) -> Result<(), DeviceSyncError>
where
    Client: ScopedGroupClient,
{
    let Some(element) = element.element else {
        return Ok(());
    };

    match element {
        Element::Consent(consent) => {
            let consent: StoredConsentRecord = consent.try_into()?;
            provider.db().insert_newer_consent_record(consent)?;
        }
        Element::Group(save) => {
            if let Ok(Some(_)) = provider.db().find_group(&save.id) {
                // Do not restore groups that already exist.
                return Ok(());
            }

            let attributes = save
                .mutable_metadata
                .map(|m| m.attributes)
                .unwrap_or_default();

            MlsGroup::insert(
                client,
                Some(&save.id),
                GroupMembershipState::Restored,
                PolicySet::default(),
                GroupMetadataOptions {
                    name: attributes.get("group_name").cloned(),
                    image_url_square: attributes.get("group_image_url_square").cloned(),
                    description: attributes.get("description").cloned(),
                    ..Default::default()
                },
            )?;
        }
        Element::GroupMessage(message) => {
            let message: StoredGroupMessage = message.try_into()?;
            message.store_or_ignore(provider.db())?;
        }
        _ => {}
    }

    Ok(())
}

pub async fn run<Client>(&mut self, client: &Client) -> Result<(), ArchiveError>
where
    Client: ScopedGroupClient,
{
    while let Some(element) = self.next_element().await? {
        match insert(element, client, client.mls_provider()) {
            Err(ArchiveError::Deserialization(err)) => {
                tracing::warn!("Unable to insert record: {err:?}");
            }
            Err(err) => return Err(err)?,
            _ => {}
        }
    }

    Ok(())
}
