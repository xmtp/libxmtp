const xmtp = require("./pkg/bindings_wasm.js");

async function run() {
  
  let client = new xmtp.WasmXmtpClient("http://localhost:5555");
  console.log(client); 
}

run();
