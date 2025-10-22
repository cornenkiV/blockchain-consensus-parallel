import json
import csv
import os
from typing import List, Dict, Any


def ensure_output_dir(directory: str = "output"):
    os.makedirs(directory, exist_ok=True)


def get_config_suffix(
    difficulty: int, num_blocks: int, txs_per_block: int = None, num_workers: int = None
) -> str:
    parts = [f"d{difficulty}", f"b{num_blocks}"]
    if txs_per_block is not None:
        parts.append(f"t{txs_per_block}")
    if num_workers is not None:
        parts.append(f"w{num_workers}")
    return "_".join(parts)


def save_json(data: Any, filename: str):
    try:
        ensure_output_dir()
        filepath = os.path.join("output", filename)
        with open(filepath, "w", encoding="utf-8") as f:
            json.dump(data, f, indent=2, ensure_ascii=False)
        print(f"  Saved: {filepath}")
    except Exception as e:
        print(f"  Error saving {filename}: {e}")


def save_csv(data: List[Dict[str, Any]], filename: str):
    try:
        if not data:
            print(f"  No data to save to {filename}")
            return

        ensure_output_dir()
        filepath = os.path.join("output", filename)

        with open(filepath, "w", newline="", encoding="utf-8") as f:
            writer = csv.DictWriter(f, fieldnames=data[0].keys())
            writer.writeheader()
            writer.writerows(data)

        print(f"  Saved: {filepath}")
    except Exception as e:
        print(f"  Error saving {filename}: {e}")


def format_hash_rate(hashes_per_second: float) -> str:
    if hashes_per_second >= 1_000_000:
        return f"{hashes_per_second / 1_000_000:.2f} MH/s"
    elif hashes_per_second >= 1_000:
        return f"{hashes_per_second / 1_000:.2f} KH/s"
    else:
        return f"{hashes_per_second:.2f} H/s"


def format_time(seconds: float) -> str:
    if seconds >= 60:
        minutes = int(seconds // 60)
        secs = seconds % 60
        return f"{minutes}m {secs:.2f}s"
    else:
        return f"{seconds:.2f}s"


def print_block_info(block, block_number: int):
    print(f"\n{'=' * 60}")
    print(f"Block #{block_number}")
    print(f"{'=' * 60}")
    print(f"Hash:          {block.hash}")
    print(f"Previous Hash: {block.previous_hash}")
    print(f"Timestamp:     {block.timestamp}")
    print(f"Nonce:         {block.nonce}")
    print(f"Transactions:  {len(block.transactions)}")

    if len(block.transactions) <= 5:
        for i, tx in enumerate(block.transactions):
            print(
                f"  TX {i + 1}: {tx.sender[:8]}... -> {tx.recipient[:8]}..., {tx.amount:.2f}"
            )
    else:
        for i in range(3):
            tx = block.transactions[i]
            print(
                f"  TX {i + 1}: {tx.sender[:8]}... -> {tx.recipient[:8]}..., {tx.amount:.2f}"
            )
        print(f"  ... and {len(block.transactions) - 3} more transactions")


def print_mining_summary(total_time: float, total_nonces: int, blocks_mined: int):
    hash_rate = total_nonces / total_time if total_time > 0 else 0

    print(f"\n{'=' * 60}")
    print(f"Mining Summary")
    print(f"{'=' * 60}")
    print(f"Blocks Mined:     {blocks_mined}")
    print(f"Total Time:       {format_time(total_time)}")
    print(f"Nonces Tested:    {total_nonces:,}")
    print(f"Average Hash Rate: {format_hash_rate(hash_rate)}")
    print(
        f"Avg Time/Block:   {format_time(total_time / blocks_mined) if blocks_mined > 0 else 'N/A'}"
    )
    print(f"{'=' * 60}\n")
