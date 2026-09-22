import { MantineProvider } from "@mantine/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import {
  Group as XmtpGroup,
  GroupPermissionsOptions,
  type Conversation,
  type PermissionPolicySet,
} from "@xmtp/browser-sdk";
import type { ComponentProps } from "react";
import { describe, expect, it, vi } from "vitest";

import { adminPolicySet, defaultPolicySet, Permissions } from "./Permissions";

const group = (
  permissions: Promise<{
    policyType: GroupPermissionsOptions;
    policySet: PermissionPolicySet;
  }>,
) =>
  Object.assign(Object.create(XmtpGroup.prototype), {
    permissions: vi.fn().mockReturnValue(permissions),
  }) as Conversation;

const renderPermissions = (props: ComponentProps<typeof Permissions>) =>
  render(
    <MantineProvider>
      <Permissions {...props} />
    </MantineProvider>,
  );

describe("Permissions", () => {
  it("reports selected default, admin, and custom policies with their policy sets", async () => {
    const onPermissionsPolicyChange = vi.fn();
    const onPolicySetChange = vi.fn();
    const conversation = group(
      Promise.resolve({
        policyType: GroupPermissionsOptions.Default,
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
      expect(onPermissionsPolicyChange).toHaveBeenLastCalledWith(
        GroupPermissionsOptions.Default,
      );
      expect(onPolicySetChange).toHaveBeenLastCalledWith(defaultPolicySet);
    });

    fireEvent.change(policySelect, { target: { value: "1" } });
    await waitFor(() => {
      expect(onPermissionsPolicyChange).toHaveBeenLastCalledWith(
        GroupPermissionsOptions.AdminOnly,
      );
      expect(onPolicySetChange).toHaveBeenLastCalledWith(adminPolicySet);
    });

    fireEvent.change(policySelect, { target: { value: "2" } });
    await waitFor(() => {
      expect(onPermissionsPolicyChange).toHaveBeenLastCalledWith(
        GroupPermissionsOptions.CustomPolicy,
      );
    });

    fireEvent.change(policySelect, { target: { value: "0" } });
    await waitFor(() => {
      expect(onPermissionsPolicyChange).toHaveBeenLastCalledWith(
        GroupPermissionsOptions.Default,
      );
      expect(onPolicySetChange).toHaveBeenLastCalledWith(defaultPolicySet);
    });
  });
});
