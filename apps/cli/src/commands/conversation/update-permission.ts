import { Args, Flags } from "@oclif/core";
import type {
  MetadataFieldKind,
  PermissionPolicy,
  PermissionUpdateKind,
} from "@xmtp/node-sdk";

import { BaseCommand } from "@/baseCommand";
import { requireGroup } from "@/utils/conversation";

const PERMISSION_UPDATE_TYPES = {
  "add-member": "addMember",
  "remove-member": "removeMember",
  "add-admin": "addAdmin",
  "remove-admin": "removeAdmin",
  "update-metadata": "updateMetadata",
} as const;

const PERMISSION_POLICIES = {
  allow: "allow",
  deny: "deny",
  admin: "admin",
  "super-admin": "superAdmin",
} as const;

const METADATA_FIELDS = {
  "app-data": "appData",
  "group-description": "description",
  "group-name": "name",
  "group-image-url": "imageUrl",
} as const;

type PermissionUpdateKindKey = keyof typeof PERMISSION_UPDATE_TYPES;
type PermissionPolicyKey = keyof typeof PERMISSION_POLICIES;
type MetadataFieldKindKey = keyof typeof METADATA_FIELDS;

function isPermissionUpdateKindKey(
  value: string,
): value is PermissionUpdateKindKey {
  return value in PERMISSION_UPDATE_TYPES;
}

function isPermissionPolicyKey(value: string): value is PermissionPolicyKey {
  return value in PERMISSION_POLICIES;
}

function isMetadataFieldKindKey(value: string): value is MetadataFieldKindKey {
  return value in METADATA_FIELDS;
}

function parsePermissionUpdateKind(value: string): PermissionUpdateKind {
  if (!isPermissionUpdateKindKey(value)) {
    const validTypes = Object.keys(PERMISSION_UPDATE_TYPES).join(", ");
    throw new Error(
      `Invalid permission type: ${value}. Valid types: ${validTypes}`,
    );
  }
  return PERMISSION_UPDATE_TYPES[value];
}

function parsePermissionPolicy(value: string): PermissionPolicy {
  if (!isPermissionPolicyKey(value)) {
    const validPolicies = Object.keys(PERMISSION_POLICIES).join(", ");
    throw new Error(
      `Invalid permission policy: ${value}. Valid policies: ${validPolicies}`,
    );
  }
  return PERMISSION_POLICIES[value];
}

function parseMetadataFieldKind(value: string): MetadataFieldKind {
  if (!isMetadataFieldKindKey(value)) {
    const validFields = Object.keys(METADATA_FIELDS).join(", ");
    throw new Error(
      `Invalid metadata field: ${value}. Valid fields: ${validFields}`,
    );
  }
  return METADATA_FIELDS[value];
}

export default class ConversationUpdatePermission extends BaseCommand {
  static description = `Update a permission policy for a group conversation.

Updates a specific permission policy for a group conversation.
This command is only available for group conversations, not DMs.

Permission types:
- add-member: Who can add new members
- remove-member: Who can remove members
- add-admin: Who can add admins
- remove-admin: Who can remove admins
- update-metadata: Who can update group metadata (requires --metadata-field)

Policy options:
- allow: Anyone can perform this action
- deny: No one can perform this action
- admin: Only admins can perform this action
- super-admin: Only super admins can perform this action

Metadata fields (for update-metadata type):
- group-name, group-description, group-image-url, app-data

Requires super admin permissions to update permission policies.`;

  static examples = [
    {
      command:
        "<%= config.bin %> <%= command.id %> <conversation-id> --type add-member --policy admin",
      description: "Restrict adding members to admins only",
    },
    {
      command:
        "<%= config.bin %> <%= command.id %> <conversation-id> --type update-metadata --policy admin --metadata-field group-name",
      description: "Restrict group name changes to admins",
    },
  ];

  static args = {
    id: Args.string({
      description: "The conversation ID",
      required: true,
    }),
  };

  static flags = {
    ...BaseCommand.baseFlags,
    type: Flags.option({
      options: [
        "add-member",
        "remove-member",
        "add-admin",
        "remove-admin",
        "update-metadata",
      ] as const,
      description: "The permission type to update",
      required: true,
    })(),
    policy: Flags.option({
      options: ["allow", "deny", "admin", "super-admin"] as const,
      description: "The new policy for this permission",
      required: true,
    })(),
    "metadata-field": Flags.option({
      options: [
        "app-data",
        "group-description",
        "group-name",
        "group-image-url",
      ] as const,
      description:
        "Metadata field to update (required when type is update-metadata)",
    })(),
  };

  async run(): Promise<void> {
    const { args, flags } = await this.parse(ConversationUpdatePermission);
    const client = await this.initClient();

    const conversation = await client.conversations.getById(args.id);

    if (!conversation) {
      this.error(`Conversation not found: ${args.id}`);
    }

    const group = requireGroup(conversation);

    const permissionType = parsePermissionUpdateKind(flags.type);
    const policy = parsePermissionPolicy(flags.policy);

    let metadataField: MetadataFieldKind | undefined;
    if (flags["metadata-field"]) {
      metadataField = parseMetadataFieldKind(flags["metadata-field"]);
    } else if (permissionType === "updateMetadata") {
      this.error("--metadata-field is required when type is update-metadata");
    }

    await group.updatePermission(permissionType, policy, metadataField);

    this.output({
      success: true,
      conversationId: args.id,
      permissionType: flags.type,
      policy: flags.policy,
      metadataField: flags["metadata-field"] ?? null,
    });
  }
}
