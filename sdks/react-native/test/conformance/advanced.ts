import {
  Client,
  type ClientOptions,
  type Signer,
} from "@xmtp/react-native-sdk";

import { boundaryJourney, storageJourney } from "./boundary";
import { deliveryJourney } from "./delivery";
import { diagnosticsJourney, redactionJourney } from "./diagnostics";
import {
  eventClientIsolationJourney,
  eventJourney,
  eventShutdownJourney,
} from "./events";

export async function advancedJourney(
  client: Client,
  signer: Signer,
  options: ClientOptions,
): Promise<string[]> {
  const evidence = [
    ...(await boundaryJourney(client, signer, options)),
    ...(await storageJourney(client, signer, options)),
  ];
  // Isolated persistent delivery progress and consent records; no other journey
  // can accidentally consume or prepopulate the state under assertion.
  const dedicated = await Client.create(signer, {
    ...options,
    storage: {
      location: {
        dbPath: `${client.storagePath!}.delivery.db3`,
        attachmentsDir: `${client.storagePath!}.delivery.attachments`,
      },
    },
  });
  try {
    evidence.push(
      ...(await deliveryJourney(dedicated)),
      ...(await eventJourney(dedicated)),
    );
    const sibling = await Client.create(signer, {
      ...options,
      storage: {
        location: {
          dbPath: dedicated.storagePath!,
          attachmentsDir: `${dedicated.storagePath!}.attachments`,
        },
      },
    });
    try {
      evidence.push(
        ...(await eventClientIsolationJourney(dedicated, sibling)),
        ...(await eventShutdownJourney(sibling, dedicated, false)),
        ...(await eventShutdownJourney(sibling, dedicated, true)),
      );
    } finally {
      await sibling.end();
    }
  } finally {
    await dedicated.end();
  }
  evidence.push(
    ...(await redactionJourney(signer, options)),
    ...(await diagnosticsJourney()),
  );
  return evidence;
}
