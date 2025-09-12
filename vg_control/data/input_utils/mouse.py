"""
Mouse movements
"""

import json
import os
import pandas as pd

from ...constants import FPS


def get_mouse_stats(csv_path):
    """
    Process mouse movement data from a video directory containing inputs.csv
    Extracts per-frame mouse delta movements and saves as tensor chunks

    Args:
        video_dir: Path to directory containing inputs.csv
    """
    frame_duration = 1.0 / FPS

    # Load and preprocess the CSV data
    mouse_data = pd.read_csv(csv_path)

    # Find start time and normalize timestamps
    start_time = mouse_data.head(1000)[
        mouse_data.head(1000)["event_type"] == "START"
    ].iloc[-1]["timestamp"]

    mouse_data = mouse_data[mouse_data["timestamp"] >= start_time].reset_index(
        drop=True
    )
    mouse_data["timestamp"] -= start_time

    # Trim to end event if exists
    end_rows = mouse_data[mouse_data["event_type"] == "END"]
    if not end_rows.empty:
        end_time = end_rows.iloc[0]["timestamp"]
        mouse_data = mouse_data[mouse_data["timestamp"] <= end_time].reset_index(
            drop=True
        )

    # Extract mouse movement data
    mouse_moves = mouse_data[mouse_data["event_type"] == "MOUSE_MOVE"].reset_index(
        drop=True
    )
    mouse_moves["frame"] = (mouse_moves["timestamp"] // frame_duration).astype(int)

    # Parse movement deltas
    mouse_moves["event_args"] = mouse_moves["event_args"].apply(json.loads)
    mouse_moves[["dx", "dy"]] = pd.DataFrame(
        mouse_moves["event_args"].tolist(), index=mouse_moves.index
    )

    # Aggregate by frame
    frame_data = (
        mouse_moves.groupby("frame").agg({"dx": "mean", "dy": "mean"}).reset_index()
    )

    # Calculate movement statistics
    stats = {
        "overall_std": ((frame_data["dx"] ** 2 + frame_data["dy"] ** 2) ** 0.5).std(),
        "x_std": frame_data["dx"].std(),
        "y_std": frame_data["dy"].std(),
        "overall_max": ((frame_data["dx"] ** 2 + frame_data["dy"] ** 2) ** 0.5).max(),
        "max_x": frame_data["dx"].abs().max(),
        "max_y": frame_data["dy"].abs().max(),
    }

    return stats
