use crate::group_mutable_metadata::MessageDisappearingSettings;

#[derive(Default, Clone)]
pub struct GroupMetadataOptions {
    pub name: Option<String>,
    pub image_url_square: Option<String>,
    pub description: Option<String>,
    pub message_disappearing_settings: Option<MessageDisappearingSettings>,
}

#[derive(Default, Clone)]
pub struct DMMetadataOptions {
    pub message_disappearing_settings: Option<MessageDisappearingSettings>,
}
