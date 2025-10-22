#!/bin/bash

DIFFICULTY=5
BLOCKS=20
TXS=10
RUNS=30

WORKERS=(1)


for W in "${WORKERS[@]}"
do
  echo "  Testiranje sa $W radnika..."
  for i in $(seq 1 $RUNS)
  do
    SUFFIX="d${DIFFICULTY}_b${BLOCKS}_t${TXS}_w${W}_run${i}"

    python python/pow_sequential.py -d $DIFFICULTY -n $BLOCKS -t $TXS

    mv output/pow_performance_sequential_*.csv scaling/output/pow_performance_sequential_python_${SUFFIX}.csv

    echo "    Run $i/$RUNS zavrsen."
  done
done

echo "Eksperiment zavrsen."
