pub struct Topic {}

impl Topic {
    pub fn build_content_topic(name: &str) -> String {
        format!("/xmtp/0/{}/proto", name)
    }

    pub fn build_direct_message_topic_v2(random_string: &str) -> String {
        Topic::build_content_topic(&format!("m-{}", random_string))
    }
}
