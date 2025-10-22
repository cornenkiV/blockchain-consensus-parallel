import hashlib
import json
import time
import random
import string
from typing import List, Dict, Any


class Transaction:
    def __init__(
        self, sender: str, recipient: str, amount: float, timestamp: float = None
    ):
        self.sender = sender
        self.recipient = recipient
        self.amount = amount
        self.timestamp = timestamp if timestamp is not None else time.time()

    def to_dict(self) -> Dict[str, Any]:
        return {
            "sender": self.sender,
            "recipient": self.recipient,
            "amount": self.amount,
            "timestamp": self.timestamp,
        }

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "Transaction":
        return Transaction(
            sender=data["sender"],
            recipient=data["recipient"],
            amount=data["amount"],
            timestamp=data["timestamp"],
        )

    def __repr__(self) -> str:
        return f"Transaction({self.sender[:8]}... -> {self.recipient[:8]}..., {self.amount:.2f})"


class Block:
    def __init__(
        self,
        previous_hash: str,
        transactions: List[Transaction],
        timestamp: float = None,
        nonce: int = 0,
    ):
        self.previous_hash = previous_hash
        self.timestamp = timestamp if timestamp is not None else time.time()
        self.nonce = nonce
        self.transactions = transactions
        self.hash = self.calculate_hash()

    def calculate_hash(self) -> str:
        # serialize tx for hashing
        transactions_str = json.dumps(
            [tx.to_dict() for tx in self.transactions], sort_keys=True
        )
        block_string = (
            f"{self.previous_hash}{self.timestamp}{self.nonce}{transactions_str}"
        )
        return hashlib.sha256(block_string.encode()).hexdigest()

    def to_dict(self) -> Dict[str, Any]:
        return {
            "previous_hash": self.previous_hash,
            "timestamp": self.timestamp,
            "nonce": self.nonce,
            "transactions": [tx.to_dict() for tx in self.transactions],
            "hash": self.hash,
        }

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "Block":
        transactions = [Transaction.from_dict(tx) for tx in data["transactions"]]
        block = Block(
            previous_hash=data["previous_hash"],
            transactions=transactions,
            timestamp=data["timestamp"],
            nonce=data["nonce"],
        )
        block.hash = data["hash"]
        return block

    def __repr__(self) -> str:
        return f"Block(hash={self.hash[:8]}..., nonce={self.nonce}, txs={len(self.transactions)})"


class Blockchain:
    def __init__(self):
        self.chain: List[Block] = []
        self.create_genesis_block()

    def create_genesis_block(self):
        genesis_tx = Transaction(
            sender="0" * 64, recipient="0" * 64, amount=0.0, timestamp=time.time()
        )
        genesis_block = Block(
            previous_hash="0" * 64,  # 64 zeros for SHA-256
            transactions=[genesis_tx],
            timestamp=time.time(),
            nonce=0,
        )
        self.chain.append(genesis_block)

    def get_latest_block(self) -> Block:
        return self.chain[-1]

    def add_block(self, block: Block, difficulty: int) -> bool:
        if not self.validate_block(block, difficulty):
            print(f"Block validation failed")
            return False

        self.chain.append(block)
        return True

    def validate_block(self, block: Block, difficulty: int) -> bool:
        # previous hash validation
        if len(self.chain) > 0:
            expected_previous_hash = self.get_latest_block().hash
            if block.previous_hash != expected_previous_hash:
                print(
                    f"Invalid previous_hash: expected {expected_previous_hash[:16]}..., "
                    f"got {block.previous_hash[:16]}..."
                )
                return False

        # hash validation
        recalculated_hash = block.calculate_hash()
        if recalculated_hash != block.hash:
            print(
                f"Hash mismatch: expected {recalculated_hash[:16]}..., "
                f"got {block.hash[:16]}..."
            )
            return False

        # hash difficulty validation
        required_prefix = "0" * difficulty
        if not block.hash.startswith(required_prefix):
            print(
                f"Hash does not meet difficulty requirement (needs {difficulty} leading zeros)"
            )
            return False

        if not block.transactions or len(block.transactions) == 0:
            print(f"Block has no transactions")
            return False

        return True

    def to_dict(self) -> Dict[str, Any]:
        return {
            "length": len(self.chain),
            "blocks": [block.to_dict() for block in self.chain],
        }

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "Blockchain":
        blockchain = Blockchain()
        blockchain.chain = []
        for block_data in data["blocks"]:
            blockchain.chain.append(Block.from_dict(block_data))
        return blockchain

    def __repr__(self) -> str:
        return f"Blockchain(length={len(self.chain)})"


def generate_random_address() -> str:
    return "".join(random.choices("0123456789abcdef", k=64))


def generate_random_transactions(count: int = 5) -> List[Transaction]:
    transactions = []
    for _ in range(count):
        tx = Transaction(
            sender=generate_random_address(),
            recipient=generate_random_address(),
            amount=round(random.uniform(0.01, 100.0), 2),
        )
        transactions.append(tx)
    return transactions


def mine_block(
    previous_hash: str,
    transactions: List[Transaction],
    difficulty: int,
    start_nonce: int = 0,
    max_nonce: int = None,
    progress_callback=None,
) -> tuple[Block, int]:
    required_prefix = "0" * difficulty
    nonce = start_nonce
    nonces_tested = 0
    timestamp = time.time()

    while max_nonce is None or nonce < max_nonce:
        block = Block(previous_hash, transactions, timestamp, nonce)
        nonces_tested += 1

        if progress_callback and nonces_tested % 100000 == 0:
            progress_callback(nonce, block.hash)

        if block.hash.startswith(required_prefix):
            return block, nonces_tested

        nonce += 1

    return None, nonces_tested
