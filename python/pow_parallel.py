import time
import argparse
import multiprocessing as mp
from blockchain import Block, Blockchain, Transaction, generate_random_transactions
from utils import (
    save_json,
    save_csv,
    print_block_info,
    print_mining_summary,
    format_hash_rate,
    format_time,
    get_config_suffix,
)


def worker_mine(
    worker_id,
    previous_hash,
    transactions_data,
    timestamp,
    difficulty,
    start_nonce,
    step,
    found_flag,
    result_queue,
):
    transactions = [Transaction.from_dict(tx) for tx in transactions_data]
    required_prefix = "0" * difficulty
    nonce = start_nonce

    nonces_tested = 0
    check_interval = 10000

    while True:
        block = Block(previous_hash, transactions, timestamp, nonce)
        nonces_tested += 1
        if block.hash.startswith(required_prefix):
            found_flag.value = 1

            result = {
                "worker_id": worker_id,
                "found": True,
                "nonce": nonce,
                "hash": block.hash,
                "block_data": {
                    "previous_hash": block.previous_hash,
                    "timestamp": block.timestamp,
                    "nonce": block.nonce,
                    "transactions": [tx.to_dict() for tx in block.transactions],
                    "hash": block.hash,
                },
                "nonces_tested": nonces_tested,
            }

            result_queue.put(result)

            return result
        if nonces_tested % check_interval == 0:
            if found_flag.value == 1:
                result = {
                    "worker_id": worker_id,
                    "found": False,
                    "nonces_tested": nonces_tested,
                }
                result_queue.put(result)

                return result
        nonce += step


def mine_block_parallel(pool, previous_hash, transactions, difficulty, num_workers):
    timestamp = time.time()
    transactions_data = [tx.to_dict() for tx in transactions]

    manager = mp.Manager()
    found_flag = manager.Value("i", 0)
    result_queue = manager.Queue()

    async_results = []
    for worker_id in range(num_workers):
        async_result = pool.apply_async(
            worker_mine,
            args=(
                worker_id,
                previous_hash,
                transactions_data,
                timestamp,
                difficulty,
                worker_id,
                num_workers,
                found_flag,
                result_queue,
            ),
        )
        async_results.append(async_result)

    found_block = None
    total_nonces_tested = 0
    worker_stats = {i: 0 for i in range(num_workers)}

    while found_block is None:
        try:
            result = result_queue.get(timeout=0.1)
            worker_id = result["worker_id"]
            nonces_tested = result["nonces_tested"]

            worker_stats[worker_id] += nonces_tested
            total_nonces_tested += nonces_tested

            if result["found"]:
                block_data = result["block_data"]
                block_transactions = [
                    Transaction.from_dict(tx) for tx in block_data["transactions"]
                ]

                found_block = Block(
                    block_data["previous_hash"],
                    block_transactions,
                    block_data["timestamp"],
                    block_data["nonce"],
                )
                found_block.hash = block_data["hash"]

                break
        except:
            pass
    for _ in range(num_workers - 1):
        try:
            result = result_queue.get(timeout=1.0)
            worker_id = result["worker_id"]
            nonces_tested = result["nonces_tested"]

            worker_stats[worker_id] += nonces_tested
            total_nonces_tested += nonces_tested
        except:
            break
    return found_block, total_nonces_tested, worker_stats


def mine_blockchain_parallel(difficulty, num_blocks, num_workers, txs_per_block):
    print(f"Parallel PoW: d={difficulty}, blocks={num_blocks}, workers={num_workers}")

    pool = mp.Pool(processes=num_workers)
    blockchain = Blockchain()

    print(f"Pool created with {num_workers} workers")

    progress_data, performance_metrics, load_balancing_stats = [], [], []
    total_nonces_tested, total_time = 0, 0

    for i in range(num_blocks):
        block_num = i + 1
        transactions = generate_random_transactions(txs_per_block)
        previous_hash = blockchain.get_latest_block().hash

        print(f"[Block {block_num}] Mining...")
        start_time = time.time()
        block, nonces_tested, worker_stats = mine_block_parallel(
            pool, previous_hash, transactions, difficulty, num_workers
        )

        elapsed_time = time.time() - start_time
        hash_rate = nonces_tested / elapsed_time if elapsed_time > 0 else 0

        if blockchain.add_block(block, difficulty):
            print(
                f"  Nonce: {block.nonce}, Time: {format_time(elapsed_time)}, Rate: {format_hash_rate(hash_rate)}"
            )

        for worker_id in sorted(worker_stats.keys()):
            nonces = worker_stats[worker_id]
            percentage = (nonces / nonces_tested * 100) if nonces_tested > 0 else 0
            print(f"    W{worker_id}: {nonces:,} ({percentage:.1f}%)")

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
                "num_workers": num_workers,
                "nonces_tested": nonces_tested,
                "time_seconds": elapsed_time,
                "hash_rate": hash_rate,
                "timestamp": block.timestamp,
                "num_transactions": len(transactions),
            }
        )

        for worker_id, nonces in worker_stats.items():
            load_balancing_stats.append(
                {
                    "block_number": block_num,
                    "worker_id": worker_id,
                    "nonces_tested": nonces,
                    "percentage": (nonces / nonces_tested * 100)
                    if nonces_tested > 0
                    else 0,
                }
            )

        total_nonces_tested += nonces_tested
        total_time += elapsed_time

    pool.close()
    pool.join()
    print("Pool closed")

    print_mining_summary(total_time, total_nonces_tested, num_blocks)

    return {
        "blockchain": blockchain,
        "progress": progress_data,
        "performance": performance_metrics,
        "load_balancing": load_balancing_stats,
        "summary": {
            "total_time": total_time,
            "total_nonces_tested": total_nonces_tested,
            "blocks_mined": num_blocks,
            "average_hash_rate": total_nonces_tested / total_time
            if total_time > 0
            else 0,
            "difficulty": difficulty,
            "num_workers": num_workers,
            "num_blocks": num_blocks,
            "txs_per_block": txs_per_block,
        },
    }


def save_results(results, difficulty, num_blocks, num_workers, txs_per_block):
    config = get_config_suffix(difficulty, num_blocks, txs_per_block, num_workers)
    save_json(
        {"metadata": results["summary"], "blocks": results["progress"]},
        f"pow_mining_parallel_{config}.json",
    )
    save_json(results["blockchain"].to_dict(), f"pow_blockchain_parallel_{config}.json")
    save_csv(results["performance"], f"pow_performance_parallel_{config}.csv")
    save_csv(results["load_balancing"], f"pow_load_balancing_stats_{config}.csv")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("-d", "--difficulty", type=int, default=4)
    parser.add_argument("-n", "--num-blocks", type=int, default=5)
    parser.add_argument("-w", "--workers", type=int, default=4)
    parser.add_argument("-t", "--txs-per-block", type=int, default=5)
    args = parser.parse_args()

    try:
        results = mine_blockchain_parallel(
            args.difficulty, args.num_blocks, args.workers, args.txs_per_block
        )
        save_results(
            results, args.difficulty, args.num_blocks, args.workers, args.txs_per_block
        )
        print("FINAL BLOCKCHAIN")
        for idx, block in enumerate(results["blockchain"].chain):
            print_block_info(block, idx)
        print(f"Total: {len(results['blockchain'].chain)} blocks")
    except KeyboardInterrupt:
        print("Interrupted")
    except Exception as e:
        print(f"Error: {e}")
        import traceback

        traceback.print_exc()


if __name__ == "__main__":
    main()
