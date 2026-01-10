use xmtp_db::{
    StorageError,
    encrypted_store::contacts::{QueryContacts, WalletAddress},
};

use crate::context::XmtpSharedContext;

pub struct ContactWalletAddress<Context> {
    context: Context,
    id: i32,
    pub wallet_address: String,
    pub label: Option<String>,
}

impl<Context: XmtpSharedContext> ContactWalletAddress<Context> {
    pub(crate) fn new(context: Context, data: WalletAddress) -> Self {
        Self {
            context,
            id: data.id,
            wallet_address: data.wallet_address,
            label: data.label,
        }
    }

    pub fn update(
        &self,
        wallet_address: String,
        label: Option<String>,
    ) -> Result<(), StorageError> {
        self.context
            .db()
            .update_wallet_address(self.id, wallet_address, label)
    }

    pub fn delete(&self) -> Result<(), StorageError> {
        self.context.db().delete_wallet_address(self.id)
    }
}
