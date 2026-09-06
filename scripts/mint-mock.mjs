// Mint mock security tokens (devnet) to your own wallet — the stand-in for what
// production mints to a buyer on fulfillment.
// Usage: KEYPAIR=devnet-keypair.json npm run mint-mock -- 25
import { getOrCreateAssociatedTokenAccount, mintTo } from "@solana/spl-token";
import { Connection, Keypair, PublicKey, clusterApiUrl } from "@solana/web3.js";
import fs from "fs";
import os from "os";

const MOCK_MINT = new PublicKey("GfUq1PKXnGnAEvfdMSRQ7LFWzgKQK3nq7pSC3E8UwPPR");

const [amount = "100"] = process.argv.slice(2);
const rpc = process.env.RPC_URL ?? clusterApiUrl("devnet");
const keypairPath = process.env.KEYPAIR ?? os.homedir() + "/.config/solana/id.json";
const me = Keypair.fromSecretKey(new Uint8Array(JSON.parse(fs.readFileSync(keypairPath, "utf8"))));
// shared devnet-only mint authority — pays the ATA rent and signs the mint
const authority = Keypair.fromSecretKey(new Uint8Array(JSON.parse(
  fs.readFileSync(new URL("../keys/mock-mint-authority.json", import.meta.url), "utf8"))));

const connection = new Connection(rpc, "confirmed");
const ata = await getOrCreateAssociatedTokenAccount(connection, authority, MOCK_MINT, me.publicKey);
const units = BigInt(Math.round(Number(amount) * 1e6));
const sig = await mintTo(connection, authority, MOCK_MINT, ata.address, authority, units);

console.log("minted      :", amount, "mock tokens (6 dp)");
console.log("to wallet   :", me.publicKey.toBase58());
console.log("token acct  :", ata.address.toBase58());
console.log("signature   :", sig);
