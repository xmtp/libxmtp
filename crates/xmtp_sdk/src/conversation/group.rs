#[xmtp_macro::sdk_export]
impl Group {
    pub async fn peer_inbox_ids(&self) -> Result<Vec<InboxId>, XmtpError> {
        let own = InboxId::unchecked(self.inner.context.inbox_id().to_string());
        Ok(self
            .members()
            .await?
            .into_iter()
            .map(|member| member.inbox_id)
            .filter(|id| *id != own)
            .collect())
    }

    pub async fn update_name(&self, value: String) -> Result<(), XmtpError> {
        let group = self.inner.clone();
        on_sdk_worker(
            self.inner.context.clone(),
            Box::pin(async move {
                group
                    .update_group_name(value)
                    .await
                    .map_err(XmtpError::from_core)
            }),
        )
        .await
    }

    pub async fn update_description(&self, value: String) -> Result<(), XmtpError> {
        let group = self.inner.clone();
        on_sdk_worker(
            self.inner.context.clone(),
            Box::pin(async move {
                group
                    .update_group_description(value)
                    .await
                    .map_err(XmtpError::from_core)
            }),
        )
        .await
    }

    pub async fn update_image_url(&self, value: String) -> Result<(), XmtpError> {
        let group = self.inner.clone();
        on_sdk_worker(
            self.inner.context.clone(),
            Box::pin(async move {
                group
                    .update_group_image_url_square(value)
                    .await
                    .map_err(XmtpError::from_core)
            }),
        )
        .await
    }

    pub async fn update_app_data(
        &self,
        value: String,
        expected: Option<String>,
    ) -> Result<(), XmtpError> {
        let group = self.inner.clone();
        on_sdk_worker(
            self.inner.context.clone(),
            Box::pin(async move {
                group
                    .update_app_data(value, expected)
                    .await
                    .map_err(XmtpError::from_core)
            }),
        )
        .await
    }

    pub async fn update_permission(
        &self,
        kind: crate::PermissionUpdateKind,
        policy: crate::PermissionPolicy,
        metadata_field: Option<crate::MetadataFieldKind>,
    ) -> Result<(), XmtpError> {
        let group = self.inner.clone();
        let policy: xmtp_mls::groups::intents::PermissionPolicyOption = policy.try_into()?;
        // The disappearing-message setting is stored as two metadata fields
        // (from and retention). Apply the policy to both so they cannot
        // diverge.
        let fields: Option<Vec<MetadataField>> = metadata_field.map(|field| {
            if matches!(field, crate::MetadataFieldKind::Disappearing) {
                vec![
                    MetadataField::MessageDisappearFromNS,
                    MetadataField::MessageDisappearInNS,
                ]
            } else {
                vec![field.into()]
            }
        });
        on_sdk_worker(
            self.inner.context.clone(),
            Box::pin(async move {
                match fields {
                    Some(fields) => {
                        for field in fields {
                            group
                                .update_permission_policy(
                                    kind.clone().into(),
                                    policy.clone(),
                                    Some(field),
                                )
                                .await
                                .map_err(XmtpError::from_core)?;
                        }
                        Ok(())
                    }
                    None => group
                        .update_permission_policy(kind.into(), policy, None)
                        .await
                        .map_err(XmtpError::from_core),
                }
            }),
        )
        .await
    }

    pub async fn add_members(
        &self,
        members: Vec<InboxId>,
    ) -> Result<crate::MembershipResult, XmtpError> {
        let group = self.inner.clone();
        let ids = members
            .into_iter()
            .map(InboxId::into_checked)
            .collect::<Result<Vec<_>, _>>()?;
        on_sdk_worker(
            self.inner.context.clone(),
            Box::pin(async move {
                group
                    .add_members(&ids)
                    .await
                    .map_err(XmtpError::from_core)?
                    .try_into()
            }),
        )
        .await
    }

    pub async fn remove_members(&self, members: Vec<InboxId>) -> Result<(), XmtpError> {
        let group = self.inner.clone();
        let ids = members
            .into_iter()
            .map(InboxId::into_checked)
            .collect::<Result<Vec<_>, _>>()?;
        if ids
            .iter()
            .any(|id| id.starts_with("0x") || id.starts_with("0X"))
        {
            return Err(XmtpError::invalid_argument(
                "Inbox IDs cannot start with '0x'.",
            ));
        }
        on_sdk_worker(
            self.inner.context.clone(),
            Box::pin(async move {
                let refs = ids.iter().map(AsRef::as_ref).collect::<Vec<&str>>();
                group
                    .remove_members(&refs)
                    .await
                    .map_err(XmtpError::from_core)
            }),
        )
        .await
    }

    /// Add members by account identity. Hosts present this as an
    /// `addMembers` overload or union.
    pub async fn add_members_by_identity(
        &self,
        members: Vec<PublicIdentity>,
    ) -> Result<crate::MembershipResult, XmtpError> {
        let group = self.inner.clone();
        let members = members
            .iter()
            .map(PublicIdentity::to_core)
            .collect::<Result<Vec<_>, _>>()?;
        on_sdk_worker(
            self.inner.context.clone(),
            Box::pin(async move {
                group
                    .add_members_by_identity(&members)
                    .await
                    .map_err(XmtpError::from_core)?
                    .try_into()
            }),
        )
        .await
    }

    /// Remove members by account identity. Hosts present this as a
    /// `removeMembers` overload or union.
    pub async fn remove_members_by_identity(
        &self,
        members: Vec<PublicIdentity>,
    ) -> Result<(), XmtpError> {
        let group = self.inner.clone();
        let members = members
            .iter()
            .map(PublicIdentity::to_core)
            .collect::<Result<Vec<_>, _>>()?;
        on_sdk_worker(
            self.inner.context.clone(),
            Box::pin(async move {
                group
                    .remove_members_by_identity(&members)
                    .await
                    .map_err(XmtpError::from_core)
            }),
        )
        .await
    }

    pub async fn add_admin(&self, inbox_id: InboxId) -> Result<(), XmtpError> {
        self.update_admin_list(xmtp_mls::groups::UpdateAdminListType::Add, inbox_id)
            .await
    }

    pub async fn remove_admin(&self, inbox_id: InboxId) -> Result<(), XmtpError> {
        self.update_admin_list(xmtp_mls::groups::UpdateAdminListType::Remove, inbox_id)
            .await
    }

    pub async fn add_super_admin(&self, inbox_id: InboxId) -> Result<(), XmtpError> {
        self.update_admin_list(xmtp_mls::groups::UpdateAdminListType::AddSuper, inbox_id)
            .await
    }

    pub async fn remove_super_admin(&self, inbox_id: InboxId) -> Result<(), XmtpError> {
        self.update_admin_list(xmtp_mls::groups::UpdateAdminListType::RemoveSuper, inbox_id)
            .await
    }

    pub async fn is_admin(&self, inbox_id: InboxId) -> Result<bool, XmtpError> {
        let inbox_id = inbox_id.into_checked()?;
        let group = self.inner.clone();
        on_sdk_worker(self.inner.context.clone(), async move {
            group.is_admin(inbox_id).map_err(XmtpError::from_core)
        })
        .await
    }

    pub async fn is_super_admin(&self, inbox_id: InboxId) -> Result<bool, XmtpError> {
        let inbox_id = inbox_id.into_checked()?;
        let group = self.inner.clone();
        on_sdk_worker(self.inner.context.clone(), async move {
            group.is_super_admin(inbox_id).map_err(XmtpError::from_core)
        })
        .await
    }

    pub async fn list_admins(&self) -> Result<Vec<InboxId>, XmtpError> {
        let group = self.inner.clone();
        on_sdk_worker(self.inner.context.clone(), async move {
            group
                .admin_list()
                .map_err(XmtpError::from_core)?
                .into_iter()
                .map(InboxId::try_from)
                .collect()
        })
        .await
    }

    pub async fn list_super_admins(&self) -> Result<Vec<InboxId>, XmtpError> {
        let group = self.inner.clone();
        on_sdk_worker(self.inner.context.clone(), async move {
            group
                .super_admin_list()
                .map_err(XmtpError::from_core)?
                .into_iter()
                .map(InboxId::try_from)
                .collect()
        })
        .await
    }

    pub async fn request_removal(&self) -> Result<(), XmtpError> {
        let group = self.inner.clone();
        on_sdk_worker(
            self.inner.context.clone(),
            Box::pin(async move { group.leave_group().await.map_err(XmtpError::from_core) }),
        )
        .await
    }

    pub async fn membership_capabilities(
        &self,
    ) -> Result<crate::GroupMembershipCapabilities, XmtpError> {
        let group = self.inner.clone();
        on_sdk_worker(self.inner.context.clone(), async move {
            group
                .membership_capabilities()
                .await
                .map_err(XmtpError::from_core)?
                .try_into()
        })
        .await
    }
}

impl Group {
    async fn update_admin_list(
        &self,
        action: xmtp_mls::groups::UpdateAdminListType,
        inbox_id: InboxId,
    ) -> Result<(), XmtpError> {
        let inbox_id = inbox_id.into_checked()?;
        let group = self.inner.clone();
        on_sdk_worker(
            self.inner.context.clone(),
            Box::pin(async move {
                group
                    .update_admin_list(action, inbox_id)
                    .await
                    .map_err(XmtpError::from_core)
            }),
        )
        .await
    }
}

#[xmtp_macro::sdk_export]
impl Dm {
    // implements: DMS-017
    pub async fn peer_inbox_id(&self) -> Result<Option<InboxId>, XmtpError> {
        use xmtp_db::group::DmIdExt;
        let group = self.inner.clone();
        on_sdk_worker(self.inner.context.clone(), async move {
            let stored = group
                .context
                .db()
                .find_group(&group.group_id)
                .map_err(XmtpError::from_core)?;
            stored
                .and_then(|stored| stored.dm_id)
                .and_then(|id| id.other_inbox_id(group.context.inbox_id()))
                .map(InboxId::try_from)
                .transpose()
        })
        .await
    }

    pub async fn duplicate_dms(&self) -> Result<Vec<Arc<Dm>>, XmtpError> {
        let group = self.inner.clone();
        let client_key = self.client_key;
        on_sdk_worker(self.inner.context.clone(), async move {
            let groups = group.find_duplicate_dms().map_err(XmtpError::from_core)?;
            let mut result = Vec::with_capacity(groups.len());
            for inner in groups {
                result.push(Arc::new(Dm::from_core(inner, client_key).await?));
            }
            Ok(result)
        })
        .await
    }
}
