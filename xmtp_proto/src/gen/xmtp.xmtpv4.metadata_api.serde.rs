// @generated
impl serde::Serialize for GetPayerInfoRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.payer_addresses.is_empty() {
            len += 1;
        }
        if self.granularity != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.metadata_api.GetPayerInfoRequest", len)?;
        if !self.payer_addresses.is_empty() {
            struct_ser.serialize_field("payer_addresses", &self.payer_addresses)?;
        }
        if self.granularity != 0 {
            let v = PayerInfoGranularity::try_from(self.granularity)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.granularity)))?;
            struct_ser.serialize_field("granularity", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GetPayerInfoRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "payer_addresses",
            "payerAddresses",
            "granularity",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            PayerAddresses,
            Granularity,
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
                            "payerAddresses" | "payer_addresses" => Ok(GeneratedField::PayerAddresses),
                            "granularity" => Ok(GeneratedField::Granularity),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GetPayerInfoRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.metadata_api.GetPayerInfoRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GetPayerInfoRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut payer_addresses__ = None;
                let mut granularity__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::PayerAddresses => {
                            if payer_addresses__.is_some() {
                                return Err(serde::de::Error::duplicate_field("payerAddresses"));
                            }
                            payer_addresses__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Granularity => {
                            if granularity__.is_some() {
                                return Err(serde::de::Error::duplicate_field("granularity"));
                            }
                            granularity__ = Some(map_.next_value::<PayerInfoGranularity>()? as i32);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(GetPayerInfoRequest {
                    payer_addresses: payer_addresses__.unwrap_or_default(),
                    granularity: granularity__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.metadata_api.GetPayerInfoRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GetPayerInfoResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.payer_info.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.metadata_api.GetPayerInfoResponse", len)?;
        if !self.payer_info.is_empty() {
            struct_ser.serialize_field("payer_info", &self.payer_info)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GetPayerInfoResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "payer_info",
            "payerInfo",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            PayerInfo,
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
                            "payerInfo" | "payer_info" => Ok(GeneratedField::PayerInfo),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GetPayerInfoResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.metadata_api.GetPayerInfoResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GetPayerInfoResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut payer_info__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::PayerInfo => {
                            if payer_info__.is_some() {
                                return Err(serde::de::Error::duplicate_field("payerInfo"));
                            }
                            payer_info__ = Some(
                                map_.next_value::<std::collections::HashMap<_, _>>()?
                            );
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(GetPayerInfoResponse {
                    payer_info: payer_info__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.metadata_api.GetPayerInfoResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for get_payer_info_response::PayerInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.period_summaries.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.metadata_api.GetPayerInfoResponse.PayerInfo", len)?;
        if !self.period_summaries.is_empty() {
            struct_ser.serialize_field("period_summaries", &self.period_summaries)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for get_payer_info_response::PayerInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "period_summaries",
            "periodSummaries",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            PeriodSummaries,
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
                            "periodSummaries" | "period_summaries" => Ok(GeneratedField::PeriodSummaries),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = get_payer_info_response::PayerInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.metadata_api.GetPayerInfoResponse.PayerInfo")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<get_payer_info_response::PayerInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut period_summaries__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::PeriodSummaries => {
                            if period_summaries__.is_some() {
                                return Err(serde::de::Error::duplicate_field("periodSummaries"));
                            }
                            period_summaries__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(get_payer_info_response::PayerInfo {
                    period_summaries: period_summaries__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.metadata_api.GetPayerInfoResponse.PayerInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for get_payer_info_response::PeriodSummary {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.amount_spent_picodollars != 0 {
            len += 1;
        }
        if self.num_messages != 0 {
            len += 1;
        }
        if self.period_start_unix_seconds != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.metadata_api.GetPayerInfoResponse.PeriodSummary", len)?;
        if self.amount_spent_picodollars != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("amount_spent_picodollars", ToString::to_string(&self.amount_spent_picodollars).as_str())?;
        }
        if self.num_messages != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("num_messages", ToString::to_string(&self.num_messages).as_str())?;
        }
        if self.period_start_unix_seconds != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("period_start_unix_seconds", ToString::to_string(&self.period_start_unix_seconds).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for get_payer_info_response::PeriodSummary {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "amount_spent_picodollars",
            "amountSpentPicodollars",
            "num_messages",
            "numMessages",
            "period_start_unix_seconds",
            "periodStartUnixSeconds",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AmountSpentPicodollars,
            NumMessages,
            PeriodStartUnixSeconds,
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
                            "amountSpentPicodollars" | "amount_spent_picodollars" => Ok(GeneratedField::AmountSpentPicodollars),
                            "numMessages" | "num_messages" => Ok(GeneratedField::NumMessages),
                            "periodStartUnixSeconds" | "period_start_unix_seconds" => Ok(GeneratedField::PeriodStartUnixSeconds),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = get_payer_info_response::PeriodSummary;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.metadata_api.GetPayerInfoResponse.PeriodSummary")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<get_payer_info_response::PeriodSummary, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut amount_spent_picodollars__ = None;
                let mut num_messages__ = None;
                let mut period_start_unix_seconds__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AmountSpentPicodollars => {
                            if amount_spent_picodollars__.is_some() {
                                return Err(serde::de::Error::duplicate_field("amountSpentPicodollars"));
                            }
                            amount_spent_picodollars__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::NumMessages => {
                            if num_messages__.is_some() {
                                return Err(serde::de::Error::duplicate_field("numMessages"));
                            }
                            num_messages__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PeriodStartUnixSeconds => {
                            if period_start_unix_seconds__.is_some() {
                                return Err(serde::de::Error::duplicate_field("periodStartUnixSeconds"));
                            }
                            period_start_unix_seconds__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(get_payer_info_response::PeriodSummary {
                    amount_spent_picodollars: amount_spent_picodollars__.unwrap_or_default(),
                    num_messages: num_messages__.unwrap_or_default(),
                    period_start_unix_seconds: period_start_unix_seconds__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.metadata_api.GetPayerInfoResponse.PeriodSummary", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GetSyncCursorRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("xmtp.xmtpv4.metadata_api.GetSyncCursorRequest", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GetSyncCursorRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
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
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GetSyncCursorRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.metadata_api.GetSyncCursorRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GetSyncCursorRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(GetSyncCursorRequest {
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.metadata_api.GetSyncCursorRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GetSyncCursorResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.latest_sync.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.metadata_api.GetSyncCursorResponse", len)?;
        if let Some(v) = self.latest_sync.as_ref() {
            struct_ser.serialize_field("latest_sync", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GetSyncCursorResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "latest_sync",
            "latestSync",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            LatestSync,
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
                            "latestSync" | "latest_sync" => Ok(GeneratedField::LatestSync),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GetSyncCursorResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.metadata_api.GetSyncCursorResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GetSyncCursorResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut latest_sync__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::LatestSync => {
                            if latest_sync__.is_some() {
                                return Err(serde::de::Error::duplicate_field("latestSync"));
                            }
                            latest_sync__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(GetSyncCursorResponse {
                    latest_sync: latest_sync__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.metadata_api.GetSyncCursorResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GetVersionRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("xmtp.xmtpv4.metadata_api.GetVersionRequest", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GetVersionRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
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
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GetVersionRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.metadata_api.GetVersionRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GetVersionRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(GetVersionRequest {
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.metadata_api.GetVersionRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GetVersionResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.version.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.metadata_api.GetVersionResponse", len)?;
        if !self.version.is_empty() {
            struct_ser.serialize_field("version", &self.version)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GetVersionResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "version",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Version,
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
                            "version" => Ok(GeneratedField::Version),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GetVersionResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.metadata_api.GetVersionResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GetVersionResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut version__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Version => {
                            if version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("version"));
                            }
                            version__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(GetVersionResponse {
                    version: version__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.metadata_api.GetVersionResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for PayerInfoGranularity {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "PAYER_INFO_GRANULARITY_UNSPECIFIED",
            Self::Hour => "PAYER_INFO_GRANULARITY_HOUR",
            Self::Day => "PAYER_INFO_GRANULARITY_DAY",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for PayerInfoGranularity {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "PAYER_INFO_GRANULARITY_UNSPECIFIED",
            "PAYER_INFO_GRANULARITY_HOUR",
            "PAYER_INFO_GRANULARITY_DAY",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PayerInfoGranularity;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "PAYER_INFO_GRANULARITY_UNSPECIFIED" => Ok(PayerInfoGranularity::Unspecified),
                    "PAYER_INFO_GRANULARITY_HOUR" => Ok(PayerInfoGranularity::Hour),
                    "PAYER_INFO_GRANULARITY_DAY" => Ok(PayerInfoGranularity::Day),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
