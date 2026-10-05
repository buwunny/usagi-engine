import numpy as np
import pytest

import usagi as mj


def random_action(mask, rng):
    """A uniformly random legal action per row of `mask`."""
    noise = rng.random(mask.shape) * mask
    return noise.argmax(axis=-1)


def test_game_plays_to_the_end():
    rng = np.random.default_rng(0)
    g = mj.Game(seed=1)
    assert g.phase == "turn" and g.current_seat == 0
    planes, scalars, scores = g.observation(0)
    assert planes.shape == (mj.NUM_PLANES, 34) and planes.dtype == np.uint8
    assert scalars.shape == (mj.NUM_SCALARS,)
    assert list(scores) == [25000] * 4
    assert planes[:4].sum() == 14  # the hand planes hold one entry per tile
    steps = 0
    while not g.done:
        if g.phase == "round_end":
            g.next_round()
            continue
        seat = g.current_seat
        legal = g.legal_actions(seat)
        assert legal and g.legal_mask(seat).sum() == len(legal)
        g.step(seat, int(rng.choice(legal)))
        steps += 1
    assert sum(g.scores) % 1000 == 0
    assert sorted(g.ranks()) == [0, 1, 2, 3]
    assert steps > 100


def test_illegal_action_raises():
    g = mj.Game(seed=0)
    illegal = next(i for i in range(mj.NUM_ACTIONS) if i not in g.legal_actions(0))
    with pytest.raises(ValueError):
        g.step(0, illegal)
    with pytest.raises(ValueError):
        g.step(1, g.legal_actions(0)[0])


def test_copy_is_independent():
    g = mj.Game(seed=3)
    h = g.copy()
    g.step(0, g.legal_actions(0)[0])
    assert h.phase == "turn" and h.current_seat == 0
    assert g.phase != "turn" or g.current_seat != 0


def test_vecenv_runs_a_thousand_random_games():
    rng = np.random.default_rng(0)
    env = mj.VecEnv(1000, seed=0)
    finished = np.zeros(1000, dtype=bool)
    finals = []
    while not finished.all():
        seats, planes, scalars, scores, masks = env.observe()
        assert planes.shape == (1000, mj.NUM_PLANES, 34)
        assert masks.shape == (1000, mj.NUM_ACTIONS)
        assert (masks.sum(axis=1) > 0).all()
        done, final = env.step(random_action(masks, rng))
        new = done & ~finished
        finals.extend(final[new])
        finished |= done
    finals = np.array(finals)
    assert len(finals) == 1000
    assert (finals.sum(axis=1) % 1000 == 0).all()


def test_vecenv_rejects_illegal_actions():
    env = mj.VecEnv(2)
    _, _, _, _, masks = env.observe()
    bad = np.array([masks[0].argmin(), masks[1].argmax()], dtype=np.int64)
    with pytest.raises(ValueError, match="env 0"):
        env.step(bad)
