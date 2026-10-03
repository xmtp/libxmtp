import { createRegisteredClient, createSigner } from "@test/helpers";
import { Timestamp } from "@xmtp/node-sdk";
import { describe, expect, it } from "vitest";

describe("Group permissions", () => {
  it("should create a group with default permissions", async () => {
    const { signer: signer1 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const group = await client1.conversations.createGroup([]);
    expect((await group.state()).permissions.policyType).toBe("allMembers");
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
  });

  it("should create a group with admin only permissions", async () => {
    const { signer: signer1 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const group = await client1.conversations.createGroup([], {
      permissions: { kind: "adminOnly" },
    });
    expect((await group.state()).permissions.policyType).toBe("adminOnly");
    expect((await group.state()).permissions.policySet).toEqual({
      addMember: "admin",
      removeMember: "admin",
      addAdmin: "superAdmin",
      removeAdmin: "superAdmin",
      updateName: "admin",
      updateDescription: "admin",
      updateImage: "admin",
      updateDisappearing: "admin",
      updateAppData: "admin",
    });
  });

  it("should create a group with custom permissions", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const group = await client1.conversations.createGroup([client2.inboxId], {
      permissions: {
        kind: "custom",
        policySet: {
          addAdmin: "deny",
          addMember: "allow",
          removeAdmin: "deny",
          removeMember: "deny",
          updateName: "deny",
          updateDescription: "deny",
          updateImage: "deny",
          updateDisappearing: "admin",
          updateAppData: "deny",
        },
      },
    });
    expect((await group.state()).permissions.policyType).toBe("custom");
    expect((await group.state()).permissions.policySet).toEqual({
      addAdmin: "deny",
      addMember: "allow",
      removeAdmin: "deny",
      removeMember: "deny",
      updateName: "deny",
      updateDescription: "deny",
      updateImage: "deny",
      updateDisappearing: "admin",
      updateAppData: "deny",
    });
  });

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

  it("should enforce add member policy", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const { signer: signer3 } = createSigner();
    const { signer: signer4 } = createSigner();
    const { signer: signer5 } = createSigner();
    const { signer: signer6 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const client3 = await createRegisteredClient(signer3);
    const client4 = await createRegisteredClient(signer4);
    const client5 = await createRegisteredClient(signer5);
    const client6 = await createRegisteredClient(signer6);
    // create group with default permissions (anyone can add members)
    const group = await client1.conversations.createGroup([client2.inboxId]);

    // client2 is a regular member of the group
    await client2.conversations.sync();
    const group2 = (await client2.conversations.listGroups({}))[0];
    // adding a member is allowed
    await group2.addMembers([client3.inboxId]);
    expect(await group2.members()).toHaveLength(3);

    // update group permissions to allow only admins to add members
    await group.updatePermission("addMember", "admin", undefined);
    // client2 is no longer able to add members
    await expect(() => group2.addMembers([client4.inboxId])).rejects.toThrow();

    // add client2 as an admin
    await group.addAdmin(client2.inboxId);
    // client2 is now able to add members
    await group2.addMembers([client4.inboxId]);
    expect(await group2.members()).toHaveLength(4);

    // update group permissions to allow only super admins to add members
    await group.updatePermission("addMember", "superAdmin", undefined);
    // client2 is no longer able to add members
    await expect(() => group2.addMembers([client5.inboxId])).rejects.toThrow();

    // add client2 as a super admin
    await group.addSuperAdmin(client2.inboxId);
    // client2 is now able to add members
    await group2.addMembers([client5.inboxId]);
    expect(await group2.members()).toHaveLength(5);

    // update group permissions to deny adding members
    await group.updatePermission("addMember", "deny", undefined);
    // client2 is no longer able to add members
    await expect(() => group2.addMembers([client6.inboxId])).rejects.toThrow();

    // update group permissions to allow anyone to add members
    await group.updatePermission("addMember", "allow", undefined);
    // client2 is able to add members again
    await group2.addMembers([client6.inboxId]);
    expect(await group2.members()).toHaveLength(6);
  });

  it("should enforce remove member policy", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const { signer: signer3 } = createSigner();
    const { signer: signer4 } = createSigner();
    const { signer: signer5 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const client3 = await createRegisteredClient(signer3);
    const client4 = await createRegisteredClient(signer4);
    const client5 = await createRegisteredClient(signer5);
    // create group with default permissions (only admins can remove members)
    const group = await client1.conversations.createGroup([
      client2.inboxId,
      client3.inboxId,
      client4.inboxId,
      client5.inboxId,
    ]);

    // client2 is a regular member of the group
    await client2.conversations.sync();
    const group2 = (await client2.conversations.listGroups({}))[0];
    // removing a member is not allowed
    await expect(() =>
      group2.removeMembers([client3.inboxId]),
    ).rejects.toThrow();

    // add client2 as an admin
    await group.addAdmin(client2.inboxId);
    // client2 is now able to remove members
    await group2.removeMembers([client5.inboxId]);
    expect(await group2.members()).toHaveLength(4);

    // update group permissions to allow only super admins to remove members
    await group.updatePermission("removeMember", "superAdmin", undefined);
    // client2 is no longer able to remove members
    await expect(() =>
      group2.removeMembers([client4.inboxId]),
    ).rejects.toThrow();

    // add client2 as a super admin
    await group.addSuperAdmin(client2.inboxId);
    // client2 is now able to remove members
    await group2.removeMembers([client4.inboxId]);
    expect(await group2.members()).toHaveLength(3);

    // update group permissions to deny removing members
    await group.updatePermission("removeMember", "deny", undefined);
    // client2 is no longer able to remove members
    await expect(() =>
      group2.removeMembers([client3.inboxId]),
    ).rejects.toThrow();

    // update group permissions to allow anyone to remove members
    await group.updatePermission("removeMember", "allow", undefined);
    // client2 is able to remove members again
    await group2.removeMembers([client3.inboxId]);
    expect(await group2.members()).toHaveLength(2);
  });

  it("should enforce add admin policy", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const { signer: signer3 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const client3 = await createRegisteredClient(signer3);
    // create group with default permissions (only super admins can add admins)
    const group = await client1.conversations.createGroup([
      client2.inboxId,
      client3.inboxId,
    ]);

    // client2 is a regular member of the group
    await client2.conversations.sync();
    const group2 = (await client2.conversations.listGroups({}))[0];

    // client2 is not a super admin, so adding an admin should fail
    await expect(() => group2.addAdmin(client3.inboxId)).rejects.toThrow();

    // add client2 as an admin
    await group.addAdmin(client2.inboxId);
    // client2 is still not able to add admins (requires super admin)
    await expect(() => group2.addAdmin(client3.inboxId)).rejects.toThrow();

    // add client2 as a super admin
    await group.addSuperAdmin(client2.inboxId);
    // now client2 should be able to add admins
    await group2.addAdmin(client3.inboxId);
  });

  it("should enforce remove admin policy", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const { signer: signer3 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const client3 = await createRegisteredClient(signer3);
    // create group with default permissions (only super admins can remove admins)
    const group = await client1.conversations.createGroup([
      client2.inboxId,
      client3.inboxId,
    ]);

    // add client3 as an admin so we have an admin to remove
    await group.addAdmin(client3.inboxId);

    // client2 is a regular member of the group
    await client2.conversations.sync();
    const group2 = (await client2.conversations.listGroups({}))[0];

    // client2 is not a super admin, so removing an admin should fail
    await expect(() => group2.removeAdmin(client3.inboxId)).rejects.toThrow();

    // add client2 as an admin (not super admin)
    await group.addAdmin(client2.inboxId);
    // client2 is still not able to remove admins (requires super admin)
    await expect(() => group2.removeAdmin(client3.inboxId)).rejects.toThrow();

    // add client2 as a super admin
    await group.addSuperAdmin(client2.inboxId);
    // now client2 should be able to remove admins
    await group2.removeAdmin(client3.inboxId);
  });

  it("should enforce update group name policy", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    // create group with default permissions (anyone can update group name)
    const group = await client1.conversations.createGroup([client2.inboxId]);

    // client2 is a regular member of the group
    await client2.conversations.sync();
    const group2 = (await client2.conversations.listGroups({}))[0];
    // updating group name is allowed
    await group2.updateName("new name 1");
    expect((await group2.state()).name).toBe("new name 1");

    // update group permissions to allow only admins to update group name
    await group.updatePermission("updateMetadata", "admin", "name");
    // client2 is no longer able to update group name
    await expect(() => group2.updateName("new name 2")).rejects.toThrow();

    // add client2 as an admin
    await group.addAdmin(client2.inboxId);
    // client2 is now able to update group name
    await group2.updateName("new name 2");
    expect((await group2.state()).name).toBe("new name 2");

    // update group permissions to allow only super admins to update group name
    await group.updatePermission("updateMetadata", "superAdmin", "name");
    // client2 is no longer able to update group name
    await expect(() => group2.updateName("new name 3")).rejects.toThrow();

    // add client2 as a super admin
    await group.addSuperAdmin(client2.inboxId);
    // client2 is now able to update group name
    await group2.updateName("new name 3");
    expect((await group2.state()).name).toBe("new name 3");

    // update group permissions to deny updating group name
    await group.updatePermission("updateMetadata", "deny", "name");
    // client2 is no longer able to update group name
    await expect(() => group2.updateName("new name 4")).rejects.toThrow();

    // update group permissions to allow anyone to update group name
    await group.updatePermission("updateMetadata", "allow", "name");

    // client2 is able to update group name again
    await group2.updateName("new name 4");
    expect((await group2.state()).name).toBe("new name 4");
  });

  it("should enforce update group description policy", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    // create group with default permissions (anyone can update group description)
    const group = await client1.conversations.createGroup([client2.inboxId]);

    // client2 is a regular member of the group
    await client2.conversations.sync();
    const group2 = (await client2.conversations.listGroups({}))[0];
    // updating group description is allowed
    await group2.updateDescription("new description 1");
    expect((await group2.state()).description).toBe("new description 1");

    // update group permissions to allow only admins to update group description
    await group.updatePermission("updateMetadata", "admin", "description");
    // client2 is no longer able to update group description
    await expect(() =>
      group2.updateDescription("new description 2"),
    ).rejects.toThrow();

    // add client2 as an admin
    await group.addAdmin(client2.inboxId);
    // client2 is now able to update group description
    await group2.updateDescription("new description 2");
    expect((await group2.state()).description).toBe("new description 2");

    // update group permissions to allow only super admins to update group description
    await group.updatePermission("updateMetadata", "superAdmin", "description");
    // client2 is no longer able to update group description
    await expect(() =>
      group2.updateDescription("new description 3"),
    ).rejects.toThrow();

    // add client2 as a super admin
    await group.addSuperAdmin(client2.inboxId);
    // client2 is now able to update group description
    await group2.updateDescription("new description 3");
    expect((await group2.state()).description).toBe("new description 3");

    // update group permissions to deny updating group description
    await group.updatePermission("updateMetadata", "deny", "description");
    // client2 is no longer able to update group description
    await expect(() =>
      group2.updateDescription("new description 4"),
    ).rejects.toThrow();

    // update group permissions to allow anyone to update group description
    await group.updatePermission("updateMetadata", "allow", "description");
    // client2 is able to update group description again
    await group2.updateDescription("new description 4");
    expect((await group2.state()).description).toBe("new description 4");
  });

  it("should enforce update group image url policy", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    // create group with default permissions (anyone can update group image url)
    const group = await client1.conversations.createGroup([client2.inboxId]);

    // client2 is a regular member of the group
    await client2.conversations.sync();
    const group2 = (await client2.conversations.listGroups({}))[0];
    // updating group image url is allowed
    await group2.updateImageUrl("https://example.com/image1.png");
    expect((await group2.state()).imageUrl).toBe(
      "https://example.com/image1.png",
    );

    // update group permissions to allow only admins to update group image url
    await group.updatePermission("updateMetadata", "admin", "imageUrl");
    // client2 is no longer able to update group image url
    await expect(() =>
      group2.updateImageUrl("https://example.com/image2.png"),
    ).rejects.toThrow();

    // add client2 as an admin
    await group.addAdmin(client2.inboxId);
    // client2 is now able to update group image url
    await group2.updateImageUrl("https://example.com/image2.png");
    expect((await group2.state()).imageUrl).toBe(
      "https://example.com/image2.png",
    );

    // update group permissions to allow only super admins to update group image url
    await group.updatePermission("updateMetadata", "superAdmin", "imageUrl");
    // client2 is no longer able to update group image url
    await expect(() =>
      group2.updateImageUrl("https://example.com/image3.png"),
    ).rejects.toThrow();

    // add client2 as a super admin
    await group.addSuperAdmin(client2.inboxId);
    // client2 is now able to update group image url
    await group2.updateImageUrl("https://example.com/image3.png");
    expect((await group2.state()).imageUrl).toBe(
      "https://example.com/image3.png",
    );

    // update group permissions to deny updating group image url
    await group.updatePermission("updateMetadata", "deny", "imageUrl");
    // client2 is no longer able to update group image url
    await expect(() =>
      group2.updateImageUrl("https://example.com/image4.png"),
    ).rejects.toThrow();

    // update group permissions to allow anyone to update group image url
    await group.updatePermission("updateMetadata", "allow", "imageUrl");
    // client2 is able to update group image url again
    await group2.updateImageUrl("https://example.com/image4.png");
    expect((await group2.state()).imageUrl).toBe(
      "https://example.com/image4.png",
    );
  });

  it("should enforce update message disappearing policy", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    // create group with default permissions (only admins can update message disappearing)
    const group = await client1.conversations.createGroup([client2.inboxId]);

    // client2 is a regular member of the group
    await client2.conversations.sync();
    const group2 = (await client2.conversations.listGroups({}))[0];
    // updating message disappearing settings is not allowed for regular members
    await expect(() =>
      group2.updateDisappearingSettings({
        from: new Timestamp(1n),
        retentionNs: 1_000_000_000n,
      }),
    ).rejects.toThrow();

    // add client2 as an admin
    await group.addAdmin(client2.inboxId);
    // client2 is now able to update message disappearing settings
    await group2.updateDisappearingSettings({
      from: new Timestamp(1n),
      retentionNs: 1_000_000_000n,
    });
    expect(
      (await group2.state()).common.disappearingSettings !== undefined,
    ).toBe(true);
  });

  it("should enforce update message disappearing policy with allow policy", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);

    // create group with custom permissions (anyone can update message disappearing)
    const group = await client1.conversations.createGroup([client2.inboxId], {
      permissions: {
        kind: "custom",
        policySet: {
          addAdmin: "superAdmin",
          addMember: "allow",
          removeAdmin: "superAdmin",
          removeMember: "admin",
          updateName: "allow",
          updateDescription: "allow",
          updateImage: "allow",
          updateDisappearing: "allow",
          updateAppData: "allow",
        },
      },
    });

    // verify permissions
    expect((await group.state()).permissions.policySet.updateDisappearing).toBe(
      "allow",
    );

    // updating message disappearing settings works
    await group.updateDisappearingSettings({
      from: new Timestamp(1n),
      retentionNs: 1_000_000_000n,
    });
    expect(
      (await group.state()).common.disappearingSettings !== undefined,
    ).toBe(true);
  });

  it("should deny update message disappearing with deny policy", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);

    // create group with custom permissions (nobody can update message disappearing)
    const group = await client1.conversations.createGroup([client2.inboxId], {
      permissions: {
        kind: "custom",
        policySet: {
          addAdmin: "superAdmin",
          addMember: "allow",
          removeAdmin: "superAdmin",
          removeMember: "admin",
          updateName: "allow",
          updateDescription: "allow",
          updateImage: "allow",
          updateDisappearing: "deny",
          updateAppData: "allow",
        },
      },
    });

    // even super admin (client1) cannot update message disappearing settings
    await expect(() =>
      group.updateDisappearingSettings({
        from: new Timestamp(1n),
        retentionNs: 1_000_000_000n,
      }),
    ).rejects.toThrow();
  });

  it("should enforce update app data policy", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    // create group with default permissions (anyone can update app data)
    const group = await client1.conversations.createGroup([client2.inboxId]);

    // client2 is a regular member of the group
    await client2.conversations.sync();
    const group2 = (await client2.conversations.listGroups({}))[0];
    // updating app data is allowed
    await group2.updateAppData("app data 1", undefined);
    expect((await group2.state()).appData).toBe("app data 1");

    // update group permissions to allow only admins to update app data
    await group.updatePermission("updateMetadata", "admin", "appData");
    // client2 is no longer able to update app data
    await expect(() =>
      group2.updateAppData("app data 2", undefined),
    ).rejects.toThrow();

    // add client2 as an admin
    await group.addAdmin(client2.inboxId);
    // client2 is now able to update app data
    await group2.updateAppData("app data 2", undefined);
    expect((await group2.state()).appData).toBe("app data 2");

    // update group permissions to allow only super admins to update app data
    await group.updatePermission("updateMetadata", "superAdmin", "appData");
    // client2 is no longer able to update app data
    await expect(() =>
      group2.updateAppData("app data 3", undefined),
    ).rejects.toThrow();

    // add client2 as a super admin
    await group.addSuperAdmin(client2.inboxId);
    // client2 is now able to update app data
    await group2.updateAppData("app data 3", undefined);
    expect((await group2.state()).appData).toBe("app data 3");

    // update group permissions to deny updating app data
    await group.updatePermission("updateMetadata", "deny", "appData");
    // client2 is no longer able to update app data
    await expect(() =>
      group2.updateAppData("app data 4", undefined),
    ).rejects.toThrow();
  });
});
