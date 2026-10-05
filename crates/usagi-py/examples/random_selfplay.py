"""Random self-play with VecEnv: how a training loop drives usagi engine.

    pip install ./crates/usagi-py
    python crates/usagi-py/examples/random_selfplay.py
"""

import time

import numpy as np

import usagi as mj

N = 1024
env = mj.VecEnv(N, seed=0)
rng = np.random.default_rng(0)
games = steps = 0
start = time.time()
while games < 2 * N:
    seats, planes, scalars, scores, masks = env.observe()
    # A policy network would read planes/scalars here; we pick at random
    # among the legal actions.
    actions = (rng.random(masks.shape) * masks).argmax(axis=1)
    done, final_scores = env.step(actions)
    games += int(done.sum())
    steps += N
elapsed = time.time() - start
print(f"{games} games, {steps} decisions in {elapsed:.1f}s "
      f"({steps / elapsed:,.0f} decisions/s, {games / elapsed:,.0f} games/s)")
