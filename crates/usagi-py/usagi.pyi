"""Fast Tenhou-rules Riichi Mahjong engine (Rust) for Python."""

from typing import Literal, Optional

import numpy as np
import numpy.typing as npt

NUM_ACTIONS: int
"""Size of the action space (155)."""
NUM_PLANES: int
"""Tile planes per observation (each 34 wide)."""
NUM_SCALARS: int
"""Scalar features per observation."""
OBS_VERSION: int
"""Observation format version."""

Phase = Literal["turn", "after_call", "call", "chankan", "round_end", "game_end"]

class Game:
    """One game of four-player Tenhou-rules Mahjong."""

    def __init__(self, seed: int = 0) -> None: ...
    def reset(self, seed: int) -> None:
        """Starts a new game from `seed`."""
    def copy(self) -> "Game":
        """A copy of this game (for search or rollouts)."""
    @property
    def phase(self) -> Phase: ...
    @property
    def waiting(self) -> list[int]:
        """Seats that must decide now (several during a call window)."""
    @property
    def current_seat(self) -> Optional[int]:
        """The next seat to act, or None between hands and after the game."""
    @property
    def scores(self) -> list[int]: ...
    @property
    def done(self) -> bool: ...
    def ranks(self) -> list[int]:
        """Final placement of each seat, 0 = first."""
    def legal_actions(self, seat: int) -> list[int]:
        """Legal action indices for `seat`."""
    def legal_mask(self, seat: int) -> npt.NDArray[np.uint8]:
        """0/1 mask of shape [NUM_ACTIONS]."""
    def describe(self, seat: int, index: int) -> Optional[str]:
        """A readable name for action `index` if it is legal for `seat`."""
    def step(self, seat: int, index: int) -> None:
        """`seat` takes action `index`. Raises ValueError if it isn't legal."""
    def next_round(self) -> None:
        """Deals the next hand; only valid when phase is "round_end"."""
    def observation(
        self, seat: int
    ) -> tuple[npt.NDArray[np.uint8], npt.NDArray[np.uint8], npt.NDArray[np.int32]]:
        """(planes [NUM_PLANES, 34], scalars [NUM_SCALARS], scores [4]) for `seat`."""

class VecEnv:
    """`num_envs` games stepped in parallel on Rust threads.

    Every game is always waiting for one seat's decision: hands are dealt
    automatically, and a finished game restarts with the next seed.
    """

    def __init__(self, num_envs: int, seed: int = 0) -> None: ...
    @property
    def num_envs(self) -> int: ...
    def observe(
        self,
    ) -> tuple[
        npt.NDArray[np.uint8],
        npt.NDArray[np.uint8],
        npt.NDArray[np.uint8],
        npt.NDArray[np.int32],
        npt.NDArray[np.uint8],
    ]:
        """(seats [N], planes [N, NUM_PLANES, 34], scalars [N, NUM_SCALARS],
        scores [N, 4], masks [N, NUM_ACTIONS]) for each game's deciding seat."""
    def step(
        self, actions: npt.NDArray[np.int64]
    ) -> tuple[npt.NDArray[np.bool_], npt.NDArray[np.int32]]:
        """Applies one action per game. Returns (done [N], final_scores [N, 4])."""
