// Example: place a buy order on the devnet orders-lite program.
// Usage: KEYPAIR=~/my-devnet-keypair.json npm run place-order -- AAPL 2500
import anchorPkg from "@coral-xyz/anchor";
const { AnchorProvider, Program, Wallet, BN } = anchorPkg;
import { Connection, Keypair, PublicKey, clusterApiUrl } from "@solana/web3.js";
import fs from "fs";
import os from "os";

const [ticker = "AAPL", usdc = "2500"] = process.argv.slice(2);
const rpc = process.env.RPC_URL ?? clusterApiUrl("devnet");
const keypairPath = (process.env.KEYPAIR ?? os.homedir() + "/.config/solana/id.json");
const payer = Keypair.fromSecretKey(new Uint8Array(JSON.parse(fs.readFileSync(keypairPath, "utf8"))));

const idl = JSON.parse(fs.readFileSync(new URL("../idl/orders_lite.json", import.meta.url), "utf8"));
const provider = new AnchorProvider(new Connection(rpc, "confirmed"), new Wallet(payer), { commitment: "confirmed" });
const program = new Program(idl, provider);

// order_id is per-user; production assigns it from the backend DB — a timestamp is fine here
const orderId = new BN(Date.now());
const [pda] = PublicKey.findProgramAddressSync(
  [Buffer.from("order"), payer.publicKey.toBuffer(), orderId.toArrayLike(Buffer, "le", 8)],
  program.programId
);

const sig = await program.methods
  .placeBuyOrder(orderId, ticker, new BN(Math.round(Number(usdc) * 1e6)))
  .accounts({ user: payer.publicKey })
  .rpc();

console.log("user        :", payer.publicKey.toBase58());
console.log("order_id    :", orderId.toString());
console.log("pending PDA :", pda.toBase58());
console.log("signature   :", sig);
