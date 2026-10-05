import { MantineProvider } from "@mantine/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import {
  Group as XmtpGroup,
  type GroupPolicyType,
  type Conversation,
  type PermissionPolicySet,
} from "@xmtp/browser-sdk";
import type { ComponentProps } from "react";
import { describe, expect, it, vi } from "vitest";

import {
  adminPolicySet,
  defaultPolicySet,
  Permissions,
  processPermissionsUpdate,
} from "./Permissions";

const group = (
  permissions: Promise<{
    policyType: GroupPolicyType;
    policySet: PermissionPolicySet;
  }>,
) =>
  Object.assign(Object.create(XmtpGroup.prototype), {
    state: vi
      .fn()
      .mockReturnValue(permissions.then((value) => ({ permissions: value }))),
  }) as Conversation;

const renderPermissions = (props: ComponentProps<typeof Permissions>) =>
  render(
    <MantineProvider>
      <Permissions {...props} />
    </MantineProvider>,
  );

describe("Permissions", () => {
  it("saves a changed custom field without submitting an existing other policy", async () => {
    const current: PermissionPolicySet = {
      ...defaultPolicySet,
      addMember: "other",
    };
    const conversation = Object.assign(Object.create(XmtpGroup.prototype), {
      state: vi.fn().mockResolvedValue({
        permissions: { policyType: "custom", policySet: current },
      }),
      updatePermission: vi.fn().mockImplementation(async (_kind, value) => {
        if (value === "other") throw new Error("unsupported policy update");
      }),
    }) as Conversation & { updatePermission: ReturnType<typeof vi.fn> };

    await processPermissionsUpdate(conversation, "custom", {
      ...current,
      updateName: "admin",
    });

    expect(conversation.updatePermission).toHaveBeenCalledTimes(1);
    expect(conversation.updatePermission).toHaveBeenCalledWith(
      "updateMetadata",
      "admin",
      "name",
    );
  });

  it("reports selected default, admin, and custom policies with their policy sets", async () => {
    const onPermissionsPolicyChange = vi.fn();
    const onPolicySetChange = vi.fn();
    const conversation = group(
      Promise.resolve({
        policyType: "allMembers",
        policySet: defaultPolicySet,
      }),
    );

    renderPermissions({
      conversation,
      onPermissionsPolicyChange,
      onPolicySetChange,
    });

    const policySelect = screen.getAllByRole("combobox")[0];
    await waitFor(() => {
      expect(onPermissionsPolicyChange).toHaveBeenLastCalledWith("allMembers");
      expect(onPolicySetChange).toHaveBeenLastCalledWith(defaultPolicySet);
    });

    fireEvent.change(policySelect, { target: { value: "adminOnly" } });
    await waitFor(() => {
      expect(onPermissionsPolicyChange).toHaveBeenLastCalledWith("adminOnly");
      expect(onPolicySetChange).toHaveBeenLastCalledWith(adminPolicySet);
    });

    fireEvent.change(policySelect, { target: { value: "custom" } });
    await waitFor(() => {
      expect(onPermissionsPolicyChange).toHaveBeenLastCalledWith("custom");
    });

    fireEvent.change(policySelect, { target: { value: "allMembers" } });
    await waitFor(() => {
      expect(onPermissionsPolicyChange).toHaveBeenLastCalledWith("allMembers");
      expect(onPolicySetChange).toHaveBeenLastCalledWith(defaultPolicySet);
    });
  });
});
