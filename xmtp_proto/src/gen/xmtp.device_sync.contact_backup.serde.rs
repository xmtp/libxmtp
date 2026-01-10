impl serde::Serialize for AddressSave {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.address1.is_some() {
            len += 1;
        }
        if self.address2.is_some() {
            len += 1;
        }
        if self.address3.is_some() {
            len += 1;
        }
        if self.city.is_some() {
            len += 1;
        }
        if self.region.is_some() {
            len += 1;
        }
        if self.postal_code.is_some() {
            len += 1;
        }
        if self.country.is_some() {
            len += 1;
        }
        if self.label.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.contact_backup.AddressSave", len)?;
        if let Some(v) = self.address1.as_ref() {
            struct_ser.serialize_field("address1", v)?;
        }
        if let Some(v) = self.address2.as_ref() {
            struct_ser.serialize_field("address2", v)?;
        }
        if let Some(v) = self.address3.as_ref() {
            struct_ser.serialize_field("address3", v)?;
        }
        if let Some(v) = self.city.as_ref() {
            struct_ser.serialize_field("city", v)?;
        }
        if let Some(v) = self.region.as_ref() {
            struct_ser.serialize_field("region", v)?;
        }
        if let Some(v) = self.postal_code.as_ref() {
            struct_ser.serialize_field("postal_code", v)?;
        }
        if let Some(v) = self.country.as_ref() {
            struct_ser.serialize_field("country", v)?;
        }
        if let Some(v) = self.label.as_ref() {
            struct_ser.serialize_field("label", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AddressSave {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "address1",
            "address2",
            "address3",
            "city",
            "region",
            "postal_code",
            "postalCode",
            "country",
            "label",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Address1,
            Address2,
            Address3,
            City,
            Region,
            PostalCode,
            Country,
            Label,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "address1" => Ok(GeneratedField::Address1),
                            "address2" => Ok(GeneratedField::Address2),
                            "address3" => Ok(GeneratedField::Address3),
                            "city" => Ok(GeneratedField::City),
                            "region" => Ok(GeneratedField::Region),
                            "postalCode" | "postal_code" => Ok(GeneratedField::PostalCode),
                            "country" => Ok(GeneratedField::Country),
                            "label" => Ok(GeneratedField::Label),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AddressSave;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.contact_backup.AddressSave")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AddressSave, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut address1__ = None;
                let mut address2__ = None;
                let mut address3__ = None;
                let mut city__ = None;
                let mut region__ = None;
                let mut postal_code__ = None;
                let mut country__ = None;
                let mut label__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Address1 => {
                            if address1__.is_some() {
                                return Err(serde::de::Error::duplicate_field("address1"));
                            }
                            address1__ = map_.next_value()?;
                        }
                        GeneratedField::Address2 => {
                            if address2__.is_some() {
                                return Err(serde::de::Error::duplicate_field("address2"));
                            }
                            address2__ = map_.next_value()?;
                        }
                        GeneratedField::Address3 => {
                            if address3__.is_some() {
                                return Err(serde::de::Error::duplicate_field("address3"));
                            }
                            address3__ = map_.next_value()?;
                        }
                        GeneratedField::City => {
                            if city__.is_some() {
                                return Err(serde::de::Error::duplicate_field("city"));
                            }
                            city__ = map_.next_value()?;
                        }
                        GeneratedField::Region => {
                            if region__.is_some() {
                                return Err(serde::de::Error::duplicate_field("region"));
                            }
                            region__ = map_.next_value()?;
                        }
                        GeneratedField::PostalCode => {
                            if postal_code__.is_some() {
                                return Err(serde::de::Error::duplicate_field("postalCode"));
                            }
                            postal_code__ = map_.next_value()?;
                        }
                        GeneratedField::Country => {
                            if country__.is_some() {
                                return Err(serde::de::Error::duplicate_field("country"));
                            }
                            country__ = map_.next_value()?;
                        }
                        GeneratedField::Label => {
                            if label__.is_some() {
                                return Err(serde::de::Error::duplicate_field("label"));
                            }
                            label__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AddressSave {
                    address1: address1__,
                    address2: address2__,
                    address3: address3__,
                    city: city__,
                    region: region__,
                    postal_code: postal_code__,
                    country: country__,
                    label: label__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.contact_backup.AddressSave", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ContactSave {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.inbox_id.is_empty() {
            len += 1;
        }
        if self.display_name.is_some() {
            len += 1;
        }
        if self.first_name.is_some() {
            len += 1;
        }
        if self.last_name.is_some() {
            len += 1;
        }
        if self.prefix.is_some() {
            len += 1;
        }
        if self.suffix.is_some() {
            len += 1;
        }
        if self.company.is_some() {
            len += 1;
        }
        if self.job_title.is_some() {
            len += 1;
        }
        if self.birthday.is_some() {
            len += 1;
        }
        if self.note.is_some() {
            len += 1;
        }
        if self.image_url.is_some() {
            len += 1;
        }
        if self.is_favorite {
            len += 1;
        }
        if self.created_at_ns != 0 {
            len += 1;
        }
        if self.updated_at_ns != 0 {
            len += 1;
        }
        if !self.phone_numbers.is_empty() {
            len += 1;
        }
        if !self.emails.is_empty() {
            len += 1;
        }
        if !self.urls.is_empty() {
            len += 1;
        }
        if !self.wallet_addresses.is_empty() {
            len += 1;
        }
        if !self.addresses.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.contact_backup.ContactSave", len)?;
        if !self.inbox_id.is_empty() {
            struct_ser.serialize_field("inbox_id", &self.inbox_id)?;
        }
        if let Some(v) = self.display_name.as_ref() {
            struct_ser.serialize_field("display_name", v)?;
        }
        if let Some(v) = self.first_name.as_ref() {
            struct_ser.serialize_field("first_name", v)?;
        }
        if let Some(v) = self.last_name.as_ref() {
            struct_ser.serialize_field("last_name", v)?;
        }
        if let Some(v) = self.prefix.as_ref() {
            struct_ser.serialize_field("prefix", v)?;
        }
        if let Some(v) = self.suffix.as_ref() {
            struct_ser.serialize_field("suffix", v)?;
        }
        if let Some(v) = self.company.as_ref() {
            struct_ser.serialize_field("company", v)?;
        }
        if let Some(v) = self.job_title.as_ref() {
            struct_ser.serialize_field("job_title", v)?;
        }
        if let Some(v) = self.birthday.as_ref() {
            struct_ser.serialize_field("birthday", v)?;
        }
        if let Some(v) = self.note.as_ref() {
            struct_ser.serialize_field("note", v)?;
        }
        if let Some(v) = self.image_url.as_ref() {
            struct_ser.serialize_field("image_url", v)?;
        }
        if self.is_favorite {
            struct_ser.serialize_field("is_favorite", &self.is_favorite)?;
        }
        if self.created_at_ns != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("created_at_ns", ToString::to_string(&self.created_at_ns).as_str())?;
        }
        if self.updated_at_ns != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("updated_at_ns", ToString::to_string(&self.updated_at_ns).as_str())?;
        }
        if !self.phone_numbers.is_empty() {
            struct_ser.serialize_field("phone_numbers", &self.phone_numbers)?;
        }
        if !self.emails.is_empty() {
            struct_ser.serialize_field("emails", &self.emails)?;
        }
        if !self.urls.is_empty() {
            struct_ser.serialize_field("urls", &self.urls)?;
        }
        if !self.wallet_addresses.is_empty() {
            struct_ser.serialize_field("wallet_addresses", &self.wallet_addresses)?;
        }
        if !self.addresses.is_empty() {
            struct_ser.serialize_field("addresses", &self.addresses)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ContactSave {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "inbox_id",
            "inboxId",
            "display_name",
            "displayName",
            "first_name",
            "firstName",
            "last_name",
            "lastName",
            "prefix",
            "suffix",
            "company",
            "job_title",
            "jobTitle",
            "birthday",
            "note",
            "image_url",
            "imageUrl",
            "is_favorite",
            "isFavorite",
            "created_at_ns",
            "createdAtNs",
            "updated_at_ns",
            "updatedAtNs",
            "phone_numbers",
            "phoneNumbers",
            "emails",
            "urls",
            "wallet_addresses",
            "walletAddresses",
            "addresses",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InboxId,
            DisplayName,
            FirstName,
            LastName,
            Prefix,
            Suffix,
            Company,
            JobTitle,
            Birthday,
            Note,
            ImageUrl,
            IsFavorite,
            CreatedAtNs,
            UpdatedAtNs,
            PhoneNumbers,
            Emails,
            Urls,
            WalletAddresses,
            Addresses,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "inboxId" | "inbox_id" => Ok(GeneratedField::InboxId),
                            "displayName" | "display_name" => Ok(GeneratedField::DisplayName),
                            "firstName" | "first_name" => Ok(GeneratedField::FirstName),
                            "lastName" | "last_name" => Ok(GeneratedField::LastName),
                            "prefix" => Ok(GeneratedField::Prefix),
                            "suffix" => Ok(GeneratedField::Suffix),
                            "company" => Ok(GeneratedField::Company),
                            "jobTitle" | "job_title" => Ok(GeneratedField::JobTitle),
                            "birthday" => Ok(GeneratedField::Birthday),
                            "note" => Ok(GeneratedField::Note),
                            "imageUrl" | "image_url" => Ok(GeneratedField::ImageUrl),
                            "isFavorite" | "is_favorite" => Ok(GeneratedField::IsFavorite),
                            "createdAtNs" | "created_at_ns" => Ok(GeneratedField::CreatedAtNs),
                            "updatedAtNs" | "updated_at_ns" => Ok(GeneratedField::UpdatedAtNs),
                            "phoneNumbers" | "phone_numbers" => Ok(GeneratedField::PhoneNumbers),
                            "emails" => Ok(GeneratedField::Emails),
                            "urls" => Ok(GeneratedField::Urls),
                            "walletAddresses" | "wallet_addresses" => Ok(GeneratedField::WalletAddresses),
                            "addresses" => Ok(GeneratedField::Addresses),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ContactSave;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.contact_backup.ContactSave")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ContactSave, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut inbox_id__ = None;
                let mut display_name__ = None;
                let mut first_name__ = None;
                let mut last_name__ = None;
                let mut prefix__ = None;
                let mut suffix__ = None;
                let mut company__ = None;
                let mut job_title__ = None;
                let mut birthday__ = None;
                let mut note__ = None;
                let mut image_url__ = None;
                let mut is_favorite__ = None;
                let mut created_at_ns__ = None;
                let mut updated_at_ns__ = None;
                let mut phone_numbers__ = None;
                let mut emails__ = None;
                let mut urls__ = None;
                let mut wallet_addresses__ = None;
                let mut addresses__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::InboxId => {
                            if inbox_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inboxId"));
                            }
                            inbox_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::DisplayName => {
                            if display_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("displayName"));
                            }
                            display_name__ = map_.next_value()?;
                        }
                        GeneratedField::FirstName => {
                            if first_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("firstName"));
                            }
                            first_name__ = map_.next_value()?;
                        }
                        GeneratedField::LastName => {
                            if last_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("lastName"));
                            }
                            last_name__ = map_.next_value()?;
                        }
                        GeneratedField::Prefix => {
                            if prefix__.is_some() {
                                return Err(serde::de::Error::duplicate_field("prefix"));
                            }
                            prefix__ = map_.next_value()?;
                        }
                        GeneratedField::Suffix => {
                            if suffix__.is_some() {
                                return Err(serde::de::Error::duplicate_field("suffix"));
                            }
                            suffix__ = map_.next_value()?;
                        }
                        GeneratedField::Company => {
                            if company__.is_some() {
                                return Err(serde::de::Error::duplicate_field("company"));
                            }
                            company__ = map_.next_value()?;
                        }
                        GeneratedField::JobTitle => {
                            if job_title__.is_some() {
                                return Err(serde::de::Error::duplicate_field("jobTitle"));
                            }
                            job_title__ = map_.next_value()?;
                        }
                        GeneratedField::Birthday => {
                            if birthday__.is_some() {
                                return Err(serde::de::Error::duplicate_field("birthday"));
                            }
                            birthday__ = map_.next_value()?;
                        }
                        GeneratedField::Note => {
                            if note__.is_some() {
                                return Err(serde::de::Error::duplicate_field("note"));
                            }
                            note__ = map_.next_value()?;
                        }
                        GeneratedField::ImageUrl => {
                            if image_url__.is_some() {
                                return Err(serde::de::Error::duplicate_field("imageUrl"));
                            }
                            image_url__ = map_.next_value()?;
                        }
                        GeneratedField::IsFavorite => {
                            if is_favorite__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isFavorite"));
                            }
                            is_favorite__ = Some(map_.next_value()?);
                        }
                        GeneratedField::CreatedAtNs => {
                            if created_at_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdAtNs"));
                            }
                            created_at_ns__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::UpdatedAtNs => {
                            if updated_at_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("updatedAtNs"));
                            }
                            updated_at_ns__ =
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PhoneNumbers => {
                            if phone_numbers__.is_some() {
                                return Err(serde::de::Error::duplicate_field("phoneNumbers"));
                            }
                            phone_numbers__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Emails => {
                            if emails__.is_some() {
                                return Err(serde::de::Error::duplicate_field("emails"));
                            }
                            emails__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Urls => {
                            if urls__.is_some() {
                                return Err(serde::de::Error::duplicate_field("urls"));
                            }
                            urls__ = Some(map_.next_value()?);
                        }
                        GeneratedField::WalletAddresses => {
                            if wallet_addresses__.is_some() {
                                return Err(serde::de::Error::duplicate_field("walletAddresses"));
                            }
                            wallet_addresses__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Addresses => {
                            if addresses__.is_some() {
                                return Err(serde::de::Error::duplicate_field("addresses"));
                            }
                            addresses__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ContactSave {
                    inbox_id: inbox_id__.unwrap_or_default(),
                    display_name: display_name__,
                    first_name: first_name__,
                    last_name: last_name__,
                    prefix: prefix__,
                    suffix: suffix__,
                    company: company__,
                    job_title: job_title__,
                    birthday: birthday__,
                    note: note__,
                    image_url: image_url__,
                    is_favorite: is_favorite__.unwrap_or_default(),
                    created_at_ns: created_at_ns__.unwrap_or_default(),
                    updated_at_ns: updated_at_ns__.unwrap_or_default(),
                    phone_numbers: phone_numbers__.unwrap_or_default(),
                    emails: emails__.unwrap_or_default(),
                    urls: urls__.unwrap_or_default(),
                    wallet_addresses: wallet_addresses__.unwrap_or_default(),
                    addresses: addresses__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.contact_backup.ContactSave", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EmailSave {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.email.is_empty() {
            len += 1;
        }
        if self.label.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.contact_backup.EmailSave", len)?;
        if !self.email.is_empty() {
            struct_ser.serialize_field("email", &self.email)?;
        }
        if let Some(v) = self.label.as_ref() {
            struct_ser.serialize_field("label", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EmailSave {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "email",
            "label",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Email,
            Label,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "email" => Ok(GeneratedField::Email),
                            "label" => Ok(GeneratedField::Label),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EmailSave;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.contact_backup.EmailSave")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EmailSave, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut email__ = None;
                let mut label__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Email => {
                            if email__.is_some() {
                                return Err(serde::de::Error::duplicate_field("email"));
                            }
                            email__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Label => {
                            if label__.is_some() {
                                return Err(serde::de::Error::duplicate_field("label"));
                            }
                            label__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EmailSave {
                    email: email__.unwrap_or_default(),
                    label: label__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.contact_backup.EmailSave", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for PhoneNumberSave {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.phone_number.is_empty() {
            len += 1;
        }
        if self.label.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.contact_backup.PhoneNumberSave", len)?;
        if !self.phone_number.is_empty() {
            struct_ser.serialize_field("phone_number", &self.phone_number)?;
        }
        if let Some(v) = self.label.as_ref() {
            struct_ser.serialize_field("label", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for PhoneNumberSave {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "phone_number",
            "phoneNumber",
            "label",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            PhoneNumber,
            Label,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "phoneNumber" | "phone_number" => Ok(GeneratedField::PhoneNumber),
                            "label" => Ok(GeneratedField::Label),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PhoneNumberSave;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.contact_backup.PhoneNumberSave")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<PhoneNumberSave, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut phone_number__ = None;
                let mut label__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::PhoneNumber => {
                            if phone_number__.is_some() {
                                return Err(serde::de::Error::duplicate_field("phoneNumber"));
                            }
                            phone_number__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Label => {
                            if label__.is_some() {
                                return Err(serde::de::Error::duplicate_field("label"));
                            }
                            label__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(PhoneNumberSave {
                    phone_number: phone_number__.unwrap_or_default(),
                    label: label__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.contact_backup.PhoneNumberSave", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for UrlSave {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.url.is_empty() {
            len += 1;
        }
        if self.label.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.contact_backup.UrlSave", len)?;
        if !self.url.is_empty() {
            struct_ser.serialize_field("url", &self.url)?;
        }
        if let Some(v) = self.label.as_ref() {
            struct_ser.serialize_field("label", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UrlSave {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "url",
            "label",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Url,
            Label,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "url" => Ok(GeneratedField::Url),
                            "label" => Ok(GeneratedField::Label),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = UrlSave;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.contact_backup.UrlSave")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<UrlSave, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut url__ = None;
                let mut label__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Url => {
                            if url__.is_some() {
                                return Err(serde::de::Error::duplicate_field("url"));
                            }
                            url__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Label => {
                            if label__.is_some() {
                                return Err(serde::de::Error::duplicate_field("label"));
                            }
                            label__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(UrlSave {
                    url: url__.unwrap_or_default(),
                    label: label__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.contact_backup.UrlSave", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for WalletAddressSave {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.wallet_address.is_empty() {
            len += 1;
        }
        if self.label.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.contact_backup.WalletAddressSave", len)?;
        if !self.wallet_address.is_empty() {
            struct_ser.serialize_field("wallet_address", &self.wallet_address)?;
        }
        if let Some(v) = self.label.as_ref() {
            struct_ser.serialize_field("label", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for WalletAddressSave {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "wallet_address",
            "walletAddress",
            "label",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            WalletAddress,
            Label,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "walletAddress" | "wallet_address" => Ok(GeneratedField::WalletAddress),
                            "label" => Ok(GeneratedField::Label),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = WalletAddressSave;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.contact_backup.WalletAddressSave")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<WalletAddressSave, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut wallet_address__ = None;
                let mut label__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::WalletAddress => {
                            if wallet_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("walletAddress"));
                            }
                            wallet_address__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Label => {
                            if label__.is_some() {
                                return Err(serde::de::Error::duplicate_field("label"));
                            }
                            label__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(WalletAddressSave {
                    wallet_address: wallet_address__.unwrap_or_default(),
                    label: label__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.contact_backup.WalletAddressSave", FIELDS, GeneratedVisitor)
    }
}
