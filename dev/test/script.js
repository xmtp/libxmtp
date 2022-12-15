let Client = require("@xmtp/xmtp-js").Client;
let Wallet = require("ethers").Wallet;

console.log("NODE VERSION", process.version);

// 0xf4BF19Ed562651837bc11ff975472ABd239D35B5
const keyBytes = [
  80, 7, 53, 52, 122, 163, 75, 130, 199, 86, 216, 14, 29, 2, 255, 71, 121, 51,
  165, 3, 208, 178, 193, 207, 223, 217, 75, 247, 84, 78, 204, 3,
];

async function checkAll() {
  const wallet = new Wallet(keyBytes);
  const client = await Client.create(wallet, {
    apiUrl: "http://wakunode:5555",
  });

  console.log("Listening…");

  try {
    for await (const message of await client.conversations.streamAllMessages()) {
      if (message.senderAddress === wallet.address) {
        continue;
      }

      await message.conversation.send("HI " + message.senderAddress);
      console.log(`Replied to ${message.senderAddress}`);
    }
  } catch (e) {
    console.info(`Error:`, e);
    await checkAll();
  }
}

checkAll().then(() => console.log("Done"));
