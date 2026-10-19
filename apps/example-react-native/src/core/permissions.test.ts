import assert from "node:assert/strict";
import { test } from "node:test";

import {
  type Authority,
  type GroupMember,
  type GroupPolicy,
  type GroupRole,
  KIND_LABELS,
  POLICY_LABELS,
  POLICY_ROW_TITLES,
  type PermissionWrite,
  canChangePermissionPreset,
  canChangeRole,
  canLeave,
  canRemoveMember,
  permissionPresetWrites,
  policiesFromSet,
  policyRows,
} from "./permissions.ts";

test("Admins only makes nine writes in order", () => {
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

test("Current policies rows read one label or a breakdown", () => {
  const set = {
    addMember: "allow",
    removeMember: "admin",
    addAdmin: "superAdmin",
    removeAdmin: "admin",
    updateName: "allow",
    updateDescription: "allow",
    updateImage: "admin",
    updateDisappearing: "other",
    updateAppData: "doesNotExist",
  } as const;
  const rows = policyRows(policiesFromSet(set));
  assert.deepEqual(
    rows.map((row) => row.title),
    Object.values(POLICY_ROW_TITLES),
  );
  // One policy reads as its label; SDK `other` and `doesNotExist` read as custom.
  assert.equal(rows[0]?.value, POLICY_LABELS.allow);
  assert.equal(rows[1]?.value, POLICY_LABELS.admin);
  assert.equal(rows[4]?.value, POLICY_LABELS.custom);
  assert.equal(rows[5]?.value, POLICY_LABELS.custom);
  // Mixed policies read as a breakdown, one "Label: Policy" per write.
  assert.equal(
    rows[2]?.value,
    `${KIND_LABELS.addAdmin}: ${POLICY_LABELS.superAdmin}; ${KIND_LABELS.removeAdmin}: ${POLICY_LABELS.admin}`,
  );
  assert.equal(rows[3]?.value.split("; ").length, 3);
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

test("action policies are independent", () => {
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

test("known policies respect the actor's role", () => {
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
  // The actor cannot act on itself even where everyone may act.
  const self = details("admin", [member], known.policies);
  assert.ok(!canChangeRole(self, own("admin"), "own"));
  assert.ok(!canRemoveMember(self, own("admin"), "own"));
  for (const target of [
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
