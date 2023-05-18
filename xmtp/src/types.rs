pub type Address = String;
pub type Message = String;

#[macro_export]
macro_rules! MessageReceivedHookType {
    ( $modifer:tt, $lt:lifetime) => {
        $modifer FnMut(Message) + $lt
    };
}
