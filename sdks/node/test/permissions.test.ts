import { createRegisteredClient, createSigner } from "@test/helpers";
import { Timestamp } from "@xmtp/node-sdk";
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
    expect((await group.state()).permissions.policyType).toBe("allMembers");

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

    // Each create-group option goes through generated lowering, and the
    // stored values come back through generated lifting. Rust tests check
    // the stored values, not these conversions.
    const policySet = {
      addMember: "allow",
      removeMember: "deny",
      addAdmin: "superAdmin",
      removeAdmin: "deny",
      updateName: "admin",
      updateDescription: "deny",
      updateImage: "deny",
      updateDisappearing: "admin",
      updateAppData: "deny",
    } as const;
    // A long retention: the test reads the settings, not the expiry.
    const disappearing = {
      from: new Timestamp(1n),
      retentionNs: 3_600_000_000_000n,
    };
    const optionsGroup = await client1.conversations.createGroup([], {
      name: "options",
      imageUrl: "https://example.com/options.png",
      description: "all options",
      appData: "app data",
      permissions: { kind: "custom", policySet },
      disappearing,
    });
    const state = await optionsGroup.state();
    expect(state).toMatchObject({
      name: "options",
      imageUrl: "https://example.com/options.png",
      description: "all options",
      appData: "app data",
      permissions: { policyType: "custom", policySet },
    });
    expect(state.common.disappearingSettings).toEqual(disappearing);
    expect(state.common.isDisappearingEnabled).toBe(true);
    expect(state.common.pausedForVersion).toBeUndefined();

    await optionsGroup.updateDisappearingSettings(undefined);
    expect((await optionsGroup.state()).common.disappearingSettings).toEqual({
      from: new Timestamp(0n),
      retentionNs: 0n,
    });
    await optionsGroup.updateName("renamed");
    expect((await optionsGroup.lastMessage())?.content).toMatchObject({
      kind: "groupUpdated",
      value: {
        metadataFieldChanges: [
          { fieldName: "group_name", oldValue: "options", newValue: "renamed" },
        ],
      },
    });

    const optimistic = await client1.conversations.createGroupOptimistic({
      name: "optimistic",
      permissions: { kind: "adminOnly" },
    });
    expect(await optimistic.state()).toMatchObject({
      name: "optimistic",
      permissions: { policyType: "adminOnly" },
    });
  });
});
