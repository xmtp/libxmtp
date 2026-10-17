/** Control a real PUT response through the loopback backend relay. */
export async function heldTransfer(store: string) {
  const url = `${store}/transfer/${crypto.randomUUID()}`;
  const command = async (name: string) => {
    const response = await fetch(`${url}/${name}`);
    if (!response.ok)
      throw new Error(`transfer control ${name}: ${response.status}`);
    return response;
  };
  await command("arm");
  const backend = `${url}/backend`;
  return { backend, command, url };
}
