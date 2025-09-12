import json
from .config import FPS, ROOT_DIR, SPLIT_SIZE
import pandas as pd
import os
import torch


def process_video(video_dir, return_tensor=False):
    """
    Process mouse movement data from a video directory containing inputs.csv
    Extracts per-frame mouse delta movements and saves as tensor chunks

    Args:
        video_dir: Path to directory containing inputs.csv
    """
    frame_duration = 1.0 / FPS

    csv_path = os.path.join(video_dir, "inputs.csv")
    output_dir = os.path.join(video_dir, "splits")
    os.makedirs(output_dir, exist_ok=True)

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

    # Convert to tensor
    total_frames = frame_data["frame"].max() + 1
    movement_tensor = torch.zeros((total_frames, 2), dtype=torch.bfloat16)

    for _, row in frame_data.iterrows():
        frame_idx = int(row["frame"])
        movement_tensor[frame_idx] = torch.tensor(
            [row["dx"], row["dy"]], dtype=torch.bfloat16
        )

    if return_tensor:
        return movement_tensor

    # Split into chunks and save
    for chunk_idx in range(0, total_frames, SPLIT_SIZE):
        end_idx = min(chunk_idx + SPLIT_SIZE, total_frames)
        chunk = movement_tensor[chunk_idx:end_idx]

        output_path = os.path.join(output_dir, f"{chunk_idx:08d}_mouse.pt")
        torch.save(chunk, output_path)


if __name__ == "__main__":
    for path in os.listdir(ROOT_DIR):
        print(f"Processing {path}")
        process_video(os.path.join(ROOT_DIR, path))
