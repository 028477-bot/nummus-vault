#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ANCHOR_DIR = resolve(__dirname, "..");
const PROGRAM_NAME = "nummus_vault";
const CANONICAL_PROGRAM_ID = "BaRfuBXneEAf6eFh3e7ECqNax8NyAmWHb3SkMWtSPUZw";
const OUT = resolve(ANCHOR_DIR, "idl", `${PROGRAM_NAME}.json`);

function runCargoPrintIdl() {
  const env = {
    ...process.env,
    ANCHOR_IDL_BUILD_PROGRAM_PATH: resolve(ANCHOR_DIR, "programs", PROGRAM_NAME),
    ANCHOR_IDL_BUILD_SKIP_LINT: "TRUE",
  };
  return execFileSync(
    "cargo",
    [
      "test",
      "--locked",
      "--features",
      "idl-build",
      "--package",
      PROGRAM_NAME,
      "__anchor_private_print_idl",
      "--",
      "--nocapture",
      "--test-threads=1",
    ],
    { cwd: ANCHOR_DIR, env, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 }
  );
}

function extractSections(raw, begin, end) {
  const out = [];
  const lines = raw.split("\n");
  let buf = null;
  for (const line of lines) {
    if (line.includes(begin)) {
      buf = [];
      const after = line.slice(line.indexOf(begin) + begin.length);
      if (after.trim()) buf.push(after);
      continue;
    }
    if (buf !== null && line.includes(end)) {
      out.push(buf.join("\n"));
      buf = null;
      continue;
    }
    if (buf !== null) buf.push(line);
  }
  return out.map((s) => JSON.parse(s));
}

function shortName(name) {
  if (typeof name !== "string") return name;
  const idx = name.lastIndexOf("::");
  return idx === -1 ? name : name.slice(idx + 2);
}
function normalise(node) {
  if (Array.isArray(node)) return node.map(normalise);
  if (node && typeof node === "object") {
    const out = {};
    for (const [k, v] of Object.entries(node)) {
      if (k === "name" && typeof v === "string" && v.includes("::")) {
        out[k] = shortName(v);
      } else if (k === "defined" && v && typeof v === "object" && "name" in v) {
        out[k] = { ...v, name: shortName(v.name) };
      } else {
        out[k] = normalise(v);
      }
    }
    return out;
  }
  return node;
}

function main() {
  const raw = runCargoPrintIdl();
  const [program] = extractSections(raw, "--- IDL begin program ---", "--- IDL end program ---");
  if (!program) throw new Error("no program IDL section emitted from source");
  const [errors] = extractSections(raw, "--- IDL begin errors ---", "--- IDL end errors ---");
  const events = extractSections(raw, "--- IDL begin event ---", "--- IDL end event ---");

  const idl = normalise(program);
  idl.address = CANONICAL_PROGRAM_ID;

  if (errors) idl.errors = normalise(errors);

  idl.types = idl.types || [];
  idl.events = idl.events || [];
  const typeNames = new Set(idl.types.map((t) => t.name));
  for (const ev of events) {
    const e = normalise(ev.event);
    idl.events.push(e);
    for (const t of normalise(ev.types || [])) {
      if (!typeNames.has(t.name)) {
        idl.types.push(t);
        typeNames.add(t.name);
      }
    }
  }

  const byName = (a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0);
  idl.instructions?.sort(byName);
  idl.accounts?.sort(byName);
  idl.events?.sort(byName);
  idl.types?.sort(byName);
  idl.errors?.sort((a, b) => a.code - b.code);

  const json = JSON.stringify(idl, null, 2) + "\n";

  if (process.argv.includes("--check")) {
    const current = readFileSync(OUT, "utf8");
    if (current !== json) {
      console.error("[gen-idl] committed IDL differs from source-generated IDL");
      process.exit(1);
    }
    console.log("[gen-idl] committed IDL matches source (OK)");
    return;
  }

  writeFileSync(OUT, json);
  console.log(`[gen-idl] wrote source-derived IDL -> ${OUT}`);
  console.log(
    `[gen-idl] instructions=${idl.instructions.length} accounts=${idl.accounts.length} events=${idl.events.length} errors=${idl.errors.length} types=${idl.types.length}`
  );
}

main();
