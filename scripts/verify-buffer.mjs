#!/usr/bin/env node
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";

const [bufferAddress, binaryPath] = process.argv.slice(2);
const rpcUrl = process.env.VAULT_RPC_URL;
if (!bufferAddress || !binaryPath || !rpcUrl) {
  throw new Error(
    "usage: VAULT_RPC_URL=<devnet> verify-buffer.mjs <buffer-address> <binary>",
  );
}

const response = await fetch(rpcUrl, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({
    jsonrpc: "2.0",
    id: 1,
    method: "getAccountInfo",
    params: [bufferAddress, { encoding: "base64", commitment: "confirmed" }],
  }),
});
const body = await response.json();
const value = body?.result?.value;
if (!value) throw new Error("devnet buffer account was not found");
if (value.owner !== "BPFLoaderUpgradeab1e11111111111111111111111") {
  throw new Error(`unexpected buffer owner: ${value.owner}`);
}

const BUFFER_METADATA_BYTES = 37;
const accountData = Buffer.from(value.data[0], "base64");
const payload = accountData.subarray(BUFFER_METADATA_BYTES);
const binary = await readFile(binaryPath);
const hash = (data) => createHash("sha256").update(data).digest("hex");

if (!payload.equals(binary)) {
  throw new Error(
    `buffer mismatch: local=${hash(binary)} on-chain=${hash(payload)}`,
  );
}
console.log(`[vault] OK  devnet buffer == local binary (${hash(binary)})`);