#!/usr/bin/env node
// Convert a firmware ELF to UF2, or inspect an existing UF2's address ranges.
//
// Convert: node scripts/elf-to-uf2.mjs --elf <in.elf> --family <hexId> --out <out.uf2>
// Inspect: node scripts/elf-to-uf2.mjs --inspect <file.uf2>
//
// The Glove80 bootloader accepts family 0x9807B007 (left half) or
// 0x9808B007 (right half).

import { readFileSync, writeFileSync } from "node:fs";

const UF2_MAGIC0 = 0x0a324655;
const UF2_MAGIC1 = 0x9e5d5157;
const UF2_MAGIC_END = 0x0ab16f30;
const UF2_FLAG_FAMILY_ID = 0x00002000;
const PAYLOAD_SIZE = 256;

function parseArgs(argv) {
  const args = {};
  for (let i = 2; i < argv.length; i += 1) {
    const key = argv[i];
    if (!key.startsWith("--")) throw new Error(`Unexpected argument: ${key}`);
    args[key.slice(2)] = argv[i + 1];
    i += 1;
  }
  return args;
}

function hex(n) {
  return `0x${n.toString(16)}`;
}

// Extract PT_LOAD segments from a 32-bit little-endian ELF.
function loadSegments(elf) {
  if (elf.readUInt32LE(0) !== 0x464c457f) throw new Error("Not an ELF file");
  if (elf[4] !== 1 || elf[5] !== 1) throw new Error("Expected ELF32 little-endian");
  const phoff = elf.readUInt32LE(28);
  const phentsize = elf.readUInt16LE(42);
  const phnum = elf.readUInt16LE(44);
  const segments = [];
  for (let i = 0; i < phnum; i += 1) {
    const p = phoff + i * phentsize;
    const type = elf.readUInt32LE(p);
    const offset = elf.readUInt32LE(p + 4);
    const paddr = elf.readUInt32LE(p + 12);
    const filesz = elf.readUInt32LE(p + 16);
    if (type === 1 && filesz > 0) {
      segments.push({ paddr, data: elf.subarray(offset, offset + filesz) });
    }
  }
  segments.sort((a, b) => a.paddr - b.paddr);
  if (segments.length === 0) throw new Error("No PT_LOAD segments with file data");
  return segments;
}

function toUf2(segments, familyId) {
  // Flatten into one contiguous image (gaps padded with 0xff, like flash).
  const start = segments[0].paddr;
  const end = Math.max(...segments.map((s) => s.paddr + s.data.length));
  const image = Buffer.alloc(end - start, 0xff);
  for (const s of segments) image.set(s.data, s.paddr - start);

  const numBlocks = Math.ceil(image.length / PAYLOAD_SIZE);
  const out = Buffer.alloc(numBlocks * 512);
  for (let block = 0; block < numBlocks; block += 1) {
    const chunk = image.subarray(block * PAYLOAD_SIZE, (block + 1) * PAYLOAD_SIZE);
    const b = out.subarray(block * 512, (block + 1) * 512);
    b.writeUInt32LE(UF2_MAGIC0, 0);
    b.writeUInt32LE(UF2_MAGIC1, 4);
    b.writeUInt32LE(UF2_FLAG_FAMILY_ID, 8);
    b.writeUInt32LE(start + block * PAYLOAD_SIZE, 12);
    b.writeUInt32LE(PAYLOAD_SIZE, 16);
    b.writeUInt32LE(block, 20);
    b.writeUInt32LE(numBlocks, 24);
    b.writeUInt32LE(familyId, 28);
    b.set(chunk, 32);
    b.writeUInt32LE(UF2_MAGIC_END, 508);
  }
  return { out, start, end };
}

function inspect(path) {
  const buf = readFileSync(path);
  if (buf.length % 512 !== 0) throw new Error("UF2 size is not a multiple of 512");
  const families = new Set();
  let min = Infinity;
  let max = 0;
  for (let off = 0; off < buf.length; off += 512) {
    if (buf.readUInt32LE(off) !== UF2_MAGIC0 || buf.readUInt32LE(off + 4) !== UF2_MAGIC1) {
      throw new Error(`Bad UF2 magic at block ${off / 512}`);
    }
    const flags = buf.readUInt32LE(off + 8);
    const addr = buf.readUInt32LE(off + 12);
    const size = buf.readUInt32LE(off + 16);
    if (flags & UF2_FLAG_FAMILY_ID) families.add(buf.readUInt32LE(off + 28));
    min = Math.min(min, addr);
    max = Math.max(max, addr + size);
  }
  console.log(`${path}: ${buf.length / 512} blocks`);
  console.log(`  address range: ${hex(min)}-${hex(max)}`);
  console.log(`  families: ${[...families].map(hex).join(", ") || "(none)"}`);
}

const args = parseArgs(process.argv);
if (args.inspect) {
  inspect(args.inspect);
} else {
  const { elf, family, out } = args;
  if (!elf || !family || !out) {
    console.error("Usage: elf-to-uf2.mjs --elf <in.elf> --family <hexId> --out <out.uf2>");
    console.error("       elf-to-uf2.mjs --inspect <file.uf2>");
    process.exit(1);
  }
  const familyId = Number.parseInt(family, 16);
  if (!Number.isFinite(familyId)) throw new Error(`Bad family id: ${family}`);
  const segments = loadSegments(readFileSync(elf));
  const { out: uf2, start, end } = toUf2(segments, familyId);
  writeFileSync(out, uf2);
  console.log(`${out}: ${uf2.length / 512} blocks, ${hex(start)}-${hex(end)}, family ${hex(familyId)}`);
}
