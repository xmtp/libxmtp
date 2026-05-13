// An "Action" or operation type will:
// - create group
// - each client sends message
// - invite to group
// - create identity
//
// - add all identities in xdbg to group
// - create a dm
// - upload key package
// - leave group
// - update group name
// - update app data
// - update commit log signer
// - update permission policy
// - update group description
// - update group image url square
// - update conversation message disappearing settings
// - remove conversation message disappearing settings
// - update admin list
// - update consent state
// - quietly update consent state
// - get mutable metadata
// assume that existing xdbg database exists from a potentially different version of libxmtp, so
// there will be pre-existing identities. any groups that already exist (sans dms) must have any new
// members created added to that group. any new new members created must be added to previous
// groups
// DM testing must use a single new identity, then succesfully dm with every other identity which already exists (this ensures cross-version DM compatibility)

// State Validation
// once all ops finish, a separate 'Validator' type must validate each:
// - are clients forked
// - do any clients have missing messages (gather all messages from all members of the group and
// ensure the history is consistent)
// in `test-util` of xmtp_db there is a `missing_messages` fn. collect the messages from all group
// members and run that query on each client should tell you if any messages from any client are
// missing. keep in mind identities/clients added later in the group may not have messages that were
// sent earlier then they were added. this is normal behavior and not an issue
//
//
//
// Keep each Op and validation well encapsulated. the code should be obvious and easy to add more
// ops or validation in the future. this `health.rs` file should only store the main business logic.
// ops and validation should happen within their own files/modules. as a general rule,
// `one-function-per-op` makes sense. there should be acentral point where ops are executed that
// this file calls on. it may make sense to make ops into a trait like `XmtpOp` with an async `execute` that is implemented on a struct like `UploadKeyPackage`, where every op gets its own file.
// Validation can be similiarly structured.
// that way we can easily do a for loop on each op and validation, and its clear how to add more ops
// or validations in the future.

// each op once executed succesfully gets either a green checkmark or red x. a red x should cause
// xdbg to exit with an error code, but not before executing the rest of the operations/validation
// (these checks should _not_ fail fast.)

pub struct Health {}

impl Health {
    pub fn new(opts: args::Health, network: args::BackendOpts) -> Self {
        Self { opts, network }
    }

    pub async fn run(self) -> Result<()> {
        todo!()
    }
}
