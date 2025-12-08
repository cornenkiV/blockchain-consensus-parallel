#!/bin/bash

BASE_BLOCKS=5
TXS=10
RUNS=30
DIFFICULTY=4  # Fiksni difficulty

WORKERS=(2 4 8 12)

OUTPUT_DIR="weak_scaling_blocks_rust/output"
mkdir -p "$OUTPUT_DIR"

for W in "${WORKERS[@]}"
do
  # Povećavamo broj blokova proporcionalno broju workera
  BLOCKS=$((BASE_BLOCKS * W))

  echo "  Testiranje sa $W radnika, $BLOCKS blokova i tezinom d=$DIFFICULTY..."

  for i in $(seq 1 $RUNS)
  do
    SUFFIX="d${DIFFICULTY}_b${BLOCKS}_t${TXS}_w${W}_run${i}"

    rust/target/release/blockchain-pow --mode pow-parallel --difficulty $DIFFICULTY --blocks $BLOCKS --transactions $TXS --workers $W

    GENERATED_FILE=$(ls -t output/pow_performance_parallel_rust_*.csv | head -n 1)

    if [ -f "$GENERATED_FILE" ]; then
        mv "$GENERATED_FILE" "$OUTPUT_DIR/pow_performance_parallel_rust_${SUFFIX}.csv"
        echo "    Run $i/$RUNS zavrsen."
    else
        echo "    Izlazni fajl nije pronadjen za Run $i/$RUNS."
    fi
  done
done

echo "Eksperiment zavrsen."
