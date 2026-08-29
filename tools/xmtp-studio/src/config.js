/**
 * LibXMTP Messaging Layer Security (MLS) Configuration
 */

export const XMTP_CONFIG = {
  protocol: {
    name: 'XMTP V3 (Messaging Layer Security)',
    ietfStandard: 'MLS (RFC 9420)',
    identityStandard: 'XIP-46 Multi-Wallet Key Association',
    storageEngine: 'Encrypted SQLite (xmtp_mls)',
    encryptionCipher: 'HPKE-P256-SHA256-AES128GCM',
  },
  sampleGroups: [
    {
      groupId: '0xgroup_agent_fi_alpha_99',
      name: 'AgentFi Orchestration Group',
      membersCount: 4,
      epoch: 42,
      encryptionStatus: 'MLS_E2E_ENCRYPTED',
    },
    {
      groupId: '0xgroup_dao_governance_secure',
      name: 'DAO Governance Security Stream',
      membersCount: 18,
      epoch: 128,
      encryptionStatus: 'MLS_E2E_ENCRYPTED',
    },
  ],
};
