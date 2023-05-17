pub type Address = String;
pub type Message = String;
// pub type MM = FnMut(Message) -> bool;
#[macro_export]
macro_rules! MessageReceivedHookType {
    ( $modifer:tt, $lt:lifetime) => {
        $modifer FnMut(Message) + $lt
    };
}
