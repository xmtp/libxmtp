import assert from "node:assert/strict";
import { test } from "node:test";

import {
  type Authority,
  type GroupMember,
  type GroupPolicy,
  type GroupRole,
  type PermissionWrite,
  canChangePermissionPreset,
  canChangeRole,
  canLeave,
  canRemoveMember,
  groupPolicy,
  permissionPresetWrites,
  policiesFromSet,
  policyRows,
  presetLabel,
  sdkPermissionPolicy,
} from "./permissions.ts";

test("Admins only makes nine writes in order (nineWritesInOrder)", () => {
  assert.deepEqual(permissionPresetWrites(true), [
    { kind: "addMember", policy: "admin" },
    { kind: "removeMember", policy: "admin" },
    { kind: "addAdmin", policy: "superAdmin" },
    { kind: "removeAdmin", policy: "superAdmin" },
    { kind: "updateMetadata", policy: "admin", field: "name" },
    { kind: "updateMetadata", policy: "admin", field: "description" },
    { kind: "updateMetadata", policy: "admin", field: "imageUrl" },
    { kind: "updateMetadata", policy: "admin", field: "appData" },
    { kind: "updateMetadata", policy: "admin", field: "disappearing" },
  ]);
  const all = permissionPresetWrites(false);
  assert.equal(all.length, 9);
  assert.equal(all.at(0)?.policy, "allow");
  assert.equal(all.at(1)?.policy, "admin");
  assert.equal(all.at(4)?.policy, "allow");
  assert.equal(all.at(-1)?.policy, "admin");
});

test("SDK policy mapping", () => {
  assert.equal(groupPolicy("doesNotExist"), "custom");
  assert.equal(groupPolicy("other"), "custom");
  assert.equal(groupPolicy("superAdmin"), "superAdmin");
  assert.equal(sdkPermissionPolicy("custom"), "other");
  assert.equal(sdkPermissionPolicy("allow"), "allow");
  assert.equal(presetLabel("allMembers"), "All members");
  assert.equal(presetLabel("adminOnly"), "Admins only");
  assert.equal(presetLabel("custom"), "Custom");
});

test("Current policies rows read one label or a breakdown", () => {
  const set = {
    addMember: "allow",
    removeMember: "admin",
    addAdmin: "superAdmin",
    removeAdmin: "admin",
    updateName: "allow",
    updateDescription: "allow",
    updateImage: "admin",
    updateDisappearing: "admin",
    updateAppData: "doesNotExist",
  } as const;
  assert.deepEqual(policyRows(policiesFromSet(set)), [
    { title: "Add members", value: "All members" },
    { title: "Remove members", value: "Admins" },
    {
      title: "Add or remove admins",
      value: "Add admins: Super admins; Remove admins: Admins",
    },
    {
      title: "Name, description, image",
      value: "Name: All members; Description: All members; Image: Admins",
    },
    { title: "App data", value: "Custom" },
    { title: "Disappearing messages", value: "Admins" },
  ]);
  assert.equal(policyRows([]).at(0)?.value, "—");
});

const own = (role: GroupRole): GroupMember => ({ id: "own", role });

function details(
  role: GroupRole,
  targets: readonly GroupMember[],
  policies: readonly PermissionWrite[],
): Authority {
  return { kind: "group", members: [own(role), ...targets], policies };
}

test("action policies are independent (MemberPermissionTests.actionPoliciesAreIndependent)", () => {
  for (const role of ["member", "admin", "superAdmin"] as const) {
    const member: GroupMember = { id: "target", role: "member" };
    const admin: GroupMember = { id: "admin", role: "admin" };
    const value = details(
      role,
      [member, admin],
      [
        { kind: "addAdmin", policy: "superAdmin" },
        { kind: "removeAdmin", policy: "deny" },
        { kind: "removeMember", policy: "admin" },
      ],
    );
    assert.equal(
      canChangeRole(value, member, "own"),
      role === "superAdmin",
      role,
    );
    assert.equal(canChangeRole(value, admin, "own"), false, role);
    assert.equal(
      canRemoveMember(value, member, "own"),
      role !== "member",
      role,
    );
    assert.equal(canRemoveMember(value, admin, "own"), role !== "member", role);
  }
});

test("known policies respect the actor's role (knownPoliciesRespectActorRole)", () => {
  const policies: readonly GroupPolicy[] = [
    "allow",
    "admin",
    "superAdmin",
    "deny",
    "custom",
  ];
  for (const policy of policies) {
    for (const role of ["member", "admin", "superAdmin"] as const) {
      const member: GroupMember = { id: "target", role: "member" };
      const admin: GroupMember = { id: "admin", role: "admin" };
      const value = details(
        role,
        [member, admin],
        [
          { kind: "addAdmin", policy },
          { kind: "removeAdmin", policy },
          { kind: "removeMember", policy },
        ],
      );
      const allowed =
        policy === "allow" ||
        (policy === "admin" && role !== "member") ||
        (policy === "superAdmin" && role === "superAdmin");
      assert.equal(
        canChangeRole(value, member, "own"),
        allowed,
        `${policy} ${role}`,
      );
      assert.equal(
        canChangeRole(value, admin, "own"),
        allowed,
        `${policy} ${role}`,
      );
      assert.equal(
        canRemoveMember(value, member, "own"),
        allowed,
        `${policy} ${role}`,
      );
    }
  }
});

test("unknown policies and protected targets stay read-only", () => {
  const member: GroupMember = { id: "target", role: "member" };
  const superAdmin: GroupMember = { id: "super", role: "superAdmin" };
  const unknown = details("superAdmin", [member], []);
  assert.ok(!canChangeRole(unknown, member, "own"));
  assert.ok(!canRemoveMember(unknown, member, "own"));
  const known = details(
    "superAdmin",
    [superAdmin, member],
    [
      { kind: "addAdmin", policy: "allow" },
      { kind: "removeAdmin", policy: "allow" },
      { kind: "removeMember", policy: "allow" },
    ],
  );
  for (const target of [
    own("superAdmin"),
    superAdmin,
    { id: "absent", role: "member" } as const,
  ]) {
    assert.ok(!canChangeRole(known, target, "own"), target.id);
    assert.ok(!canRemoveMember(known, target, "own"), target.id);
  }
  assert.ok(!canChangeRole(known, member, "absent"));
  assert.ok(!canRemoveMember(known, member, "absent"));
  const dm: Authority = { ...known, kind: "dm" };
  assert.ok(!canChangeRole(dm, member, "own"));
  assert.ok(!canRemoveMember(dm, member, "own"));
});

test("only super admins change the preset; super admins cannot leave", () => {
  const other: GroupMember = { id: "other", role: "superAdmin" };
  for (const role of ["member", "admin"] as const) {
    assert.ok(
      !canChangePermissionPreset(details(role, [other], []), "own"),
      role,
    );
    assert.ok(canLeave(details(role, [other], []), "own"), role);
  }
  assert.ok(
    canChangePermissionPreset(details("superAdmin", [other], []), "own"),
  );
  assert.ok(!canLeave(details("superAdmin", [other], []), "own"));
  assert.ok(
    !canChangePermissionPreset(
      { kind: "group", members: [other], policies: [] },
      "own",
    ),
  );
  assert.ok(!canLeave(details("member", [], []), "own"));
  assert.ok(
    !canLeave({ ...details("member", [other], []), pendingLeave: true }, "own"),
  );
  assert.ok(
    !canLeave({ ...details("member", [other], []), kind: "dm" }, "own"),
  );
});
