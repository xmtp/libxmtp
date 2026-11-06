/** Control a real PUT response through the loopback backend relay. */
export async function heldTransfer(store: string) {
  const id = crypto.randomUUID();
  const url = `${store}/transfer/${id}`;
  const command = async (name: string) => {
    const response = await fetch(`${url}/${name}`);
    if (!response.ok)
      throw new Error(`transfer control ${name}: ${response.status}`);
    return response;
  };
  await command("arm");
  const backend = store.replace("127.0.0.1", `${id}.transfer.localhost`);
  return { backend, command, url };
}
