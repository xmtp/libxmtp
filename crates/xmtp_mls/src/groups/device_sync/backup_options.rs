use xmtp_proto::xmtp::device_sync::{
    BackupElementSelection as BackupElementSelectionProto, BackupOptions as BackupOptionsProto,
};

/// Native representation of backup element selection with strongly-typed variants.
///
/// This wraps the proto `BackupElementSelection` to avoid working with raw `i32` values
/// and protobuf-specific conventions like `Unspecified`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BackupElementSelection {
    #[default]
    Unspecified,
    Messages,
    Consent,
    Event,
}

impl From<BackupElementSelectionProto> for BackupElementSelection {
    fn from(proto: BackupElementSelectionProto) -> Self {
        match proto {
            BackupElementSelectionProto::Unspecified => Self::Unspecified,
            BackupElementSelectionProto::Messages => Self::Messages,
            BackupElementSelectionProto::Consent => Self::Consent,
            BackupElementSelectionProto::Event => Self::Event,
        }
    }
}

impl From<BackupElementSelection> for BackupElementSelectionProto {
    fn from(selection: BackupElementSelection) -> Self {
        match selection {
            BackupElementSelection::Unspecified => Self::Unspecified,
            BackupElementSelection::Messages => Self::Messages,
            BackupElementSelection::Consent => Self::Consent,
            BackupElementSelection::Event => Self::Event,
        }
    }
}

/// Native representation of backup options with strongly-typed element selections.
///
/// This wraps the proto `BackupOptions` to provide a more ergonomic API,
/// using `BackupElementSelection` enum values directly instead of raw `i32`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct BackupOptions {
    pub elements: Vec<BackupElementSelection>,
    pub start_ns: Option<i64>,
    pub end_ns: Option<i64>,
    pub exclude_disappearing_messages: bool,
}

impl From<BackupOptionsProto> for BackupOptions {
    fn from(proto: BackupOptionsProto) -> Self {
        Self {
            elements: proto.elements().map(BackupElementSelection::from).collect(),
            start_ns: proto.start_ns,
            end_ns: proto.end_ns,
            exclude_disappearing_messages: proto.exclude_disappearing_messages,
        }
    }
}

impl From<BackupOptions> for BackupOptionsProto {
    fn from(opts: BackupOptions) -> Self {
        Self {
            elements: opts
                .elements
                .into_iter()
                .map(|e| BackupElementSelectionProto::from(e) as i32)
                .collect(),
            start_ns: opts.start_ns,
            end_ns: opts.end_ns,
            exclude_disappearing_messages: opts.exclude_disappearing_messages,
        }
    }
}

impl From<&BackupOptions> for BackupOptionsProto {
    fn from(opts: &BackupOptions) -> Self {
        Self {
            elements: opts
                .elements
                .iter()
                .map(|e| BackupElementSelectionProto::from(*e) as i32)
                .collect(),
            start_ns: opts.start_ns,
            end_ns: opts.end_ns,
            exclude_disappearing_messages: opts.exclude_disappearing_messages,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_element_selection_round_trip() {
        let variants = [
            BackupElementSelection::Unspecified,
            BackupElementSelection::Messages,
            BackupElementSelection::Consent,
            BackupElementSelection::Event,
        ];

        for variant in variants {
            let proto: BackupElementSelectionProto = variant.into();
            let round_tripped: BackupElementSelection = proto.into();
            assert_eq!(variant, round_tripped);
        }
    }

    #[test]
    fn test_options_round_trip() {
        let opts = BackupOptions {
            elements: vec![
                BackupElementSelection::Messages,
                BackupElementSelection::Consent,
            ],
            start_ns: Some(1000),
            end_ns: Some(2000),
            exclude_disappearing_messages: true,
        };

        let proto: BackupOptionsProto = opts.clone().into();
        let round_tripped: BackupOptions = proto.into();

        assert_eq!(opts, round_tripped);
    }

    #[test]
    fn test_default() {
        let selection = BackupElementSelection::default();
        assert_eq!(selection, BackupElementSelection::Unspecified);

        let opts = BackupOptions::default();
        assert!(opts.elements.is_empty());
        assert_eq!(opts.start_ns, None);
        assert_eq!(opts.end_ns, None);
        assert!(!opts.exclude_disappearing_messages);
    }

    #[test]
    fn test_from_ref() {
        let opts = BackupOptions {
            elements: vec![BackupElementSelection::Consent],
            start_ns: Some(500),
            end_ns: None,
            exclude_disappearing_messages: false,
        };

        let proto: BackupOptionsProto = (&opts).into();
        assert_eq!(
            proto.elements,
            vec![BackupElementSelectionProto::Consent as i32]
        );
        assert_eq!(proto.start_ns, Some(500));
        assert_eq!(proto.end_ns, None);
        assert!(!proto.exclude_disappearing_messages);
    }
}
