#!/bin/bash

BLOCKS=20
TXS=10
RUNS=30

WORKERS=(2 8)

OUTPUT_DIR="weak_scaling/output"
mkdir -p "$OUTPUT_DIR"


for W in "${WORKERS[@]}"
do
  DIFFICULTY=0
  case $W in
    2)  DIFFICULTY=5 ;;
    8)  DIFFICULTY=6 ;;
  esac

  if [ "$DIFFICULTY" -eq 0 ]; then
    echo "  error"
    continue
  fi

  echo "  Testiranje sa $W radnika i tezinom d=$DIFFICULTY..."
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
