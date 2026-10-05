import { createRegisteredClient, createSigner } from "@test/helpers";
import { describe, expect, it } from "vitest";

// One smoke test for the policy-set record and the update-permission
// arguments. Rust owns policy enforcement: see
// `crates/xmtp_mls/src/groups/tests/test_metadata_permissions.rs` and
// `crates/xmtp_mls_validation/src/group_permissions.rs`.
describe("Group permissions", () => {
  it("should update group permissions", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const group = await client1.conversations.createGroup([client2.inboxId]);

    expect((await group.state()).permissions.policySet).toEqual({
      addMember: "allow",
      removeMember: "admin",
      addAdmin: "superAdmin",
      removeAdmin: "superAdmin",
      updateName: "allow",
      updateDescription: "allow",
      updateImage: "allow",
      updateDisappearing: "admin",
      updateAppData: "allow",
    });

    await group.updatePermission("addMember", "admin", undefined);

    await group.updatePermission("removeMember", "superAdmin", undefined);

    await group.updatePermission("addAdmin", "admin", undefined);

    await group.updatePermission("removeAdmin", "admin", undefined);

    await group.updatePermission("updateMetadata", "admin", "name");

    await group.updatePermission("updateMetadata", "admin", "description");

    await group.updatePermission("updateMetadata", "admin", "imageUrl");

    await group.updatePermission("updateMetadata", "admin", "appData");

    expect((await group.state()).permissions.policySet).toEqual({
      addMember: "admin",
      removeMember: "superAdmin",
      addAdmin: "admin",
      removeAdmin: "admin",
      updateName: "admin",
      updateDescription: "admin",
      updateImage: "admin",
      updateDisappearing: "admin",
      updateAppData: "admin",
    });
  });
});
