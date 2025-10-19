import time
import argparse
from blockchain import Block, Blockchain, mine_block, generate_random_transactions
from utils import (
    save_json,
    save_csv,
    print_block_info,
    print_mining_summary,
    format_hash_rate,
    format_time,
    get_config_suffix,
)


def mine_blockchain_sequential(
    difficulty: int, num_blocks: int = 5, txs_per_block: int = 5
):
    print(f"\n{'=' * 60}")
    print(f"Sequential PoW Mining")
    print(f"{'=' * 60}")
    print(f"Difficulty: {difficulty} (required prefix: {'0' * difficulty})")
    print(f"Blocks to mine: {num_blocks}")
    print(f"Transactions per block: {txs_per_block}")
    print(f"{'=' * 60}\n")

    blockchain = Blockchain()
    print(f"  Genesis block created: {blockchain.get_latest_block().hash}")

    progress_data = []
    performance_metrics = []

    total_nonces_tested = 0
    total_time = 0

    for i in range(num_blocks):
        block_num = i + 1
        transactions = generate_random_transactions(txs_per_block)
        previous_hash = blockchain.get_latest_block().hash

        print(
            f"\n[Block {block_num}] Starting mining with {len(transactions)} transactions..."
        )

        nonces_checked = 0
        start_time = time.time()

        def progress_callback(nonce, current_hash):
            nonlocal nonces_checked
            nonces_checked += 100000
            elapsed = time.time() - start_time

            hash_rate = nonces_checked / elapsed if elapsed > 0 else 0
            print(
                f"  Progress: {nonces_checked:,} nonces tested, "
                f"rate: {format_hash_rate(hash_rate)}, "
                f"time: {format_time(elapsed)}"
            )

        block, nonces_tested = mine_block(
            previous_hash=previous_hash,
            transactions=transactions,
            difficulty=difficulty,
            start_nonce=0,
            max_nonce=None,
            progress_callback=progress_callback,
        )

        elapsed_time = time.time() - start_time
        hash_rate = nonces_tested / elapsed_time if elapsed_time > 0 else 0

        if blockchain.add_block(block, difficulty):
            print(
                f"  Block mined and validated. Nonce: {block.nonce}, Hash: {block.hash}"
            )
            print(
                f"  Time: {format_time(elapsed_time)}, "
                f"Nonces tested: {nonces_tested:,}, "
                f"Hash rate: {format_hash_rate(hash_rate)}"
            )
        else:
            print(f"  Block validation failed")
            continue

        progress_data.append(
            {
                "block_number": block_num,
                "nonce": block.nonce,
                "nonces_tested": nonces_tested,
                "elapsed_time": elapsed_time,
                "cumulative_time": total_time + elapsed_time,
                "hash_rate": hash_rate,
                "hash": block.hash,
                "num_transactions": len(transactions),
            }
        )

        performance_metrics.append(
            {
                "block_number": block_num,
                "difficulty": difficulty,
                "nonces_tested": nonces_tested,
                "time_seconds": elapsed_time,
                "hash_rate": hash_rate,
                "timestamp": block.timestamp,
                "num_transactions": len(transactions),
            }
        )

        total_nonces_tested += nonces_tested
        total_time += elapsed_time

    print_mining_summary(total_time, total_nonces_tested, num_blocks)

    return {
        "blockchain": blockchain,
        "progress": progress_data,
        "performance": performance_metrics,
        "summary": {
            "total_time": total_time,
            "total_nonces_tested": total_nonces_tested,
            "blocks_mined": num_blocks,
            "average_hash_rate": total_nonces_tested / total_time
            if total_time > 0
            else 0,
            "difficulty": difficulty,
            "num_blocks": num_blocks,
            "txs_per_block": txs_per_block,
        },
    }


def save_results(results: dict, difficulty: int, num_blocks: int, txs_per_block: int):
    print("\nSaving results...")

    config = get_config_suffix(difficulty, num_blocks, txs_per_block)

    progress_output = {
        "metadata": {
            "difficulty": results["summary"]["difficulty"],
            "total_blocks": results["summary"]["blocks_mined"],
            "total_time": results["summary"]["total_time"],
            "total_nonces": results["summary"]["total_nonces_tested"],
            "average_hash_rate": results["summary"]["average_hash_rate"],
        },
        "blocks": results["progress"],
    }
    save_json(progress_output, f"pow_mining_sequential_{config}.json")

    blockchain_output = results["blockchain"].to_dict()
    save_json(blockchain_output, f"pow_blockchain_sequential_{config}.json")

    save_csv(results["performance"], f"pow_performance_sequential_{config}.csv")

    print("  Results saved successfully")


def main():
    parser = argparse.ArgumentParser(
        description="Sequential Proof-of-Work blockchain mining"
    )
    parser.add_argument(
        "-d",
        "--difficulty",
        type=int,
        default=4,
        help="Mining difficulty (default: 4)",
    )
    parser.add_argument(
        "-n",
        "--num-blocks",
        type=int,
        default=5,
        help="Number of blocks to mine (default: 5)",
    )
    parser.add_argument(
        "-t",
        "--txs-per-block",
        type=int,
        default=5,
        help="Number of transactions per block (default: 5)",
    )

    args = parser.parse_args()

    if args.difficulty < 1:
        print("Difficulty must be at least 1")
        return

    if args.num_blocks < 1:
        print("Number of blocks must be at least 1")
        return

    if args.txs_per_block < 1:
        print("Number of transactions must be at least 1")
        return

    try:
        results = mine_blockchain_sequential(
            difficulty=args.difficulty,
            num_blocks=args.num_blocks,
            txs_per_block=args.txs_per_block,
        )

        save_results(results, args.difficulty, args.num_blocks, args.txs_per_block)

        print("\n" + "=" * 60)
        print("FINAL BLOCKCHAIN")
        print("=" * 60)
        for idx, block in enumerate(results["blockchain"].chain):
            print_block_info(block, idx)
        print("\n" + "=" * 60)
        print(f"Total blockchain length: {len(results['blockchain'].chain)} blocks")
        print("=" * 60)

    except KeyboardInterrupt:
        print("\n\nMining interrupted by user")
    except Exception as e:
        print(f"\n  Error during mining: {e}")
        raise


if __name__ == "__main__":
    main()
