import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const readPolicy = (name) =>
  JSON.parse(readFileSync(new URL(`../turnkey/${name}`, import.meta.url), "utf8"));

const VAULT = "BaRfuBXneEAf6eFh3e7ECqNax8NyAmWHb3SkMWtSPUZw";
const WITHDRAW = "b712469c946da122";
const LP = [
  "87802f4d0f98f031",
  "2e9cf3760dcdfbb2",
  "a026d06f685b2c01",
  "9ae6fa0decd14bdf",
  "a498cf631eba13b6",
  "7b86510031446262",
];
const ROOT = [
  "afaf6d1f0d989bed",
  "cda755ed90caf8af",
  "1d9efcbf0a53db63",
  "1494ecc64c77638e",
  "6b56c65b210c6ba0",
];

test("withdrawal template matches nonce advance then withdraw only", () => {
  const policy = readPolicy("20-withdrawal.policy.json");
  assert.match(policy.consensus, /automation/);
  assert.match(policy.consensus, /PRIIT_USER_ID/);
  assert.deepEqual(policy.constraints.requiredApprovers, ["automation", "priit"]);
  assert.match(policy.notes, /on-chain program independently binds destination/i);
  assert.match(policy.notes, /cannot be paused/i);
  assert.match(policy.condition, /instructions\.count\(\) == 2/);
  assert.match(policy.condition, /instruction_data_hex == '04000000'/);
  assert.match(policy.condition, new RegExp(WITHDRAW));
  assert.match(policy.condition, new RegExp(VAULT));
  assert.match(policy.condition, /transfers\.count\(\) == 0/);
  assert.match(policy.condition, /spl_transfers\.count\(\) == 0/);
  for (const discriminator of LP) {
    assert.doesNotMatch(policy.condition, new RegExp(discriminator));
  }
  for (const discriminator of ROOT) {
    assert.doesNotMatch(policy.condition, new RegExp(discriminator));
  }
});

test("automatic LP template permits only the six LP discriminators", () => {
  const condition = readPolicy("10-auto-liquidity.policy.json").condition.toLowerCase();
  assert.match(condition, /instructions\.count\(\) == 1/);
  for (const discriminator of LP) assert.match(condition, new RegExp(discriminator));
  assert.doesNotMatch(condition, new RegExp(WITHDRAW));
  for (const discriminator of ROOT) assert.doesNotMatch(condition, new RegExp(discriminator));
});

test("4-of-6 root template cannot authorize withdrawal or LP", () => {
  const policy = readPolicy("30-root-4of6.policy.json");
  const condition = policy.condition.toLowerCase();
  assert.match(policy.consensus, />= 4/);
  assert.match(condition, /instructions\.count\(\) == 1/);
  for (const discriminator of ROOT) assert.match(condition, new RegExp(discriminator));
  assert.match(condition, /03000000/);
  assert.match(condition, /06000000/);
  assert.doesNotMatch(condition, new RegExp(WITHDRAW));
  for (const discriminator of LP) assert.doesNotMatch(condition, new RegExp(discriminator));
});
