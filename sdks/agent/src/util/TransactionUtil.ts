import type { WalletSendCalls } from "@xmtp/node-sdk";
import {
  createPublicClient,
  encodeFunctionData,
  http,
  toHex,
  type Chain,
  type Hex,
  type Transport,
} from "viem";

/**
 * Minimal ERC-20 ABI containing transfer, balanceOf, and decimals functions.
 * Can be used with viem's encodeFunctionData for custom ERC-20 interactions.
 *
 * @see https://eips.ethereum.org/EIPS/eip-20#methods
 */
export const erc20Abi = [
  {
    /** ABI entry kind. */
    type: "function",
    /** ERC-20 function name. */
    name: "transfer",
    /** Recipient and amount parameters. */
    inputs: [
      {
        /** Recipient parameter name. */
        name: "to",
        /** Solidity address type. */
        type: "address",
      },
      {
        /** Token amount parameter name. */
        name: "amount",
        /** Unsigned 256-bit integer type. */
        type: "uint256",
      },
    ],
    /** Transfer success result. */
    outputs: [
      {
        /** The ABI does not name this return value. */
        name: "",
        /** Boolean result type. */
        type: "bool",
      },
    ],
    /** This function does not accept native tokens. */
    stateMutability: "nonpayable",
  },
  {
    /** ABI entry kind. */
    type: "function",
    /** ERC-20 balance lookup function name. */
    name: "balanceOf",
    /** Account whose balance is requested. */
    inputs: [
      {
        /** Account parameter name. */
        name: "account",
        /** Solidity address type. */
        type: "address",
      },
    ],
    /** Balance in the token's base units. */
    outputs: [
      {
        /** The ABI does not name this return value. */
        name: "",
        /** Unsigned 256-bit integer type. */
        type: "uint256",
      },
    ],
    /** This function does not change contract state. */
    stateMutability: "view",
  },
  {
    /** ABI entry kind. */
    type: "function",
    /** ERC-20 decimal precision function name. */
    name: "decimals",
    /** This function has no parameters. */
    inputs: [],
    /** Number of decimal places used by the token. */
    outputs: [
      {
        /** The ABI does not name this return value. */
        name: "",
        /** Unsigned 8-bit integer type. */
        type: "uint8",
      },
    ],
    /** This function does not change contract state. */
    stateMutability: "view",
  },
] as const;

/** Parameters for creating an ERC-20 transfer call payload. */
export type CreateERC20TransferCallsOptions = {
  /** The viem Chain object (e.g., baseSepolia from "viem/chains"). */
  chain: Chain;
  /** The ERC-20 token contract address (e.g., Base Token Contract List: https://basescan.org/tokens). */
  tokenAddress: Hex;
  /** The sender's address. */
  from: Hex;
  /** The recipient's address. */
  to: Hex;
  /** The amount to transfer in the token's base units (e.g., 1_000_000 for 1 USDC). */
  amount: bigint;
  /** Description that will be shown in the app with the transaction. */
  description: string;
};

/** Parameters for creating a native-token transfer call payload. */
export type CreateNativeTransferCallsOptions = Omit<
  CreateERC20TransferCallsOptions,
  "tokenAddress"
>;

/** Parameters for reading an ERC-20 balance. */
export type GetERC20BalanceOptions = {
  /** The viem Chain object. */
  chain: Chain;
  /** The ERC-20 token contract address. */
  tokenAddress: Hex;
  /** The address to query the balance of. */
  address: Hex;
  /** Optional custom viem transport. Defaults to http(). */
  transport?: Transport;
};

/** Parameters for reading ERC-20 token decimals. */
export type GetERC20DecimalsOptions = {
  /** The viem Chain object. */
  chain: Chain;
  /** The ERC-20 token contract address. */
  tokenAddress: Hex;
  /** Optional custom viem transport. Defaults to http(). */
  transport?: Transport;
};

/**
 * Creates a WalletSendCalls payload for an ERC-20 token transfer.
 *
 * @param options - The transfer options
 * @returns A WalletSendCalls object ready to send
 */
export function createERC20TransferCalls(
  options: CreateERC20TransferCallsOptions,
): WalletSendCalls {
  const { chain, tokenAddress, from, to, amount, description } = options;

  const data = encodeFunctionData({
    abi: erc20Abi,
    functionName: "transfer",
    args: [to, amount],
  });

  return {
    version: "1.0",
    chainId: toHex(chain.id),
    from,
    calls: [
      {
        to: tokenAddress,
        data,
        value: "0x0",
        metadata: {
          description,
          transactionType: "transfer",
        },
      },
    ],
  };
}

/**
 * Creates a WalletSendCalls payload for a native token transfer (ETH, MATIC, etc.).
 *
 * @param options - The transfer options
 * @returns A WalletSendCalls object ready to send
 */
export function createNativeTransferCalls(
  options: CreateNativeTransferCallsOptions,
): WalletSendCalls {
  const { chain, from, to, amount, description } = options;

  return {
    version: "1.0",
    chainId: toHex(chain.id),
    from,
    calls: [
      {
        to,
        value: toHex(amount),
        metadata: {
          description,
          transactionType: "transfer",
        },
      },
    ],
  };
}

/**
 * Reads the ERC-20 token balance for a given address from the blockchain.
 *
 * @param options - The query options including chain, token address, and wallet address
 * @returns The token balance in base units as a bigint
 */
export async function getERC20Balance(
  options: GetERC20BalanceOptions,
): Promise<bigint> {
  const { chain, tokenAddress, address, transport: customTransport } = options;

  const client = createPublicClient({
    chain,
    transport: customTransport ?? http(),
  });

  return client.readContract({
    address: tokenAddress,
    abi: erc20Abi,
    functionName: "balanceOf",
    args: [address],
  });
}

/**
 * Reads the number of decimals for an ERC-20 token from the blockchain.
 *
 * @param options - The query options including chain and token address
 * @returns The number of decimals (typically 6 or 18)
 */
export async function getERC20Decimals(
  options: GetERC20DecimalsOptions,
): Promise<number> {
  const { chain, tokenAddress, transport: customTransport } = options;

  const client = createPublicClient({
    chain,
    transport: customTransport ?? http(),
  });

  return client.readContract({
    address: tokenAddress,
    abi: erc20Abi,
    functionName: "decimals",
  });
}
