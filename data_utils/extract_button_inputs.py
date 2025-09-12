import json
from .config import FPS, ROOT_DIR, SPLIT_SIZE, KEYBINDS
import pandas as pd
import os
import torch

from .keybinds import CODE_TO_KEY


def get_ascii(keycode_int):
    return CODE_TO_KEY.get(keycode_int, f"Unknown key: {keycode_int}")


def get_keycode(ascii_char):
    for code, key in CODE_TO_KEY.items():
        if key == ascii_char:
            return code
    return None


def process_video(video_dir, return_tensor=False):
    """
    Process button input data from a video directory containing inputs.csv
    Extracts per-frame button states and saves as tensor chunks

    Args:
        video_dir: Path to directory containing inputs.csv
    """
    frame_duration = 1.0 / FPS
    valid_codes = [get_keycode(k) for k in KEYBINDS if (k != "LMB") and (k != "RMB")]

    csv_path = os.path.join(video_dir, "inputs.csv")
    output_dir = os.path.join(video_dir, "splits")
    os.makedirs(output_dir, exist_ok=True)

    # Load and preprocess the CSV data
    button_data = pd.read_csv(csv_path)

    # Find start time and normalize timestamps
    start_time = button_data.head(1000)[
        button_data.head(1000)["event_type"] == "START"
    ].iloc[-1]["timestamp"]

    button_data = button_data[button_data["timestamp"] >= start_time].reset_index(
        drop=True
    )
    button_data["timestamp"] -= start_time

    # Trim to end event if exists
    end_rows = button_data[button_data["event_type"] == "END"]
    if not end_rows.empty:
        end_time = end_rows.iloc[0]["timestamp"]
        button_data = button_data[button_data["timestamp"] <= end_time].reset_index(
            drop=True
        )

    # Filter for keyboard and mouse button events only
    button_data = button_data[
        button_data["event_type"].isin(["KEYBOARD", "MOUSE_BUTTON"])
    ].reset_index(drop=True)

    # Filter for keys of interest
    keyboard_mask = button_data["event_type"] == "KEYBOARD"
    keyboard_events = button_data[keyboard_mask].copy()
    keyboard_events.loc[:, "keycode"] = keyboard_events["event_args"].apply(
        lambda x: json.loads(x)[0]
    )
    button_data = button_data[
        ~(
            (button_data["event_type"] == "KEYBOARD")
            & (~keyboard_events["keycode"].isin(valid_codes))
        )
    ].reset_index(drop=True)

    # Filter for LMB or RMB only
    mouse_mask = button_data["event_type"] == "MOUSE_BUTTON"
    mouse_events = button_data[mouse_mask].copy()
    mouse_events.loc[:, "button"] = mouse_events["event_args"].apply(
        lambda x: json.loads(x)[0]
    )
    button_data = button_data[
        ~(
            (button_data["event_type"] == "MOUSE_BUTTON")
            & (~mouse_events["button"].isin([1, 2]))
        )
    ].reset_index(drop=True)

    # Convert keyboard events to UP/DOWN
    keyboard_mask = button_data["event_type"] == "KEYBOARD"
    keyboard_rows = button_data[keyboard_mask].copy()
    keyboard_rows.loc[:, "is_pressed"] = keyboard_rows["event_args"].apply(
        lambda x: json.loads(x)[1]
    )
    keyboard_rows.loc[keyboard_rows["is_pressed"], "event_type"] = "KEY_DOWN"
    keyboard_rows.loc[~keyboard_rows["is_pressed"], "event_type"] = "KEY_UP"
    keyboard_rows.loc[:, "event_args"] = keyboard_rows["event_args"].apply(
        lambda x: get_ascii(json.loads(x)[0])
    )
    button_data.loc[keyboard_mask] = keyboard_rows

    # Convert mouse events to UP/DOWN
    mouse_mask = button_data["event_type"] == "MOUSE_BUTTON"
    mouse_rows = button_data[mouse_mask].copy()
    mouse_rows.loc[:, "is_pressed"] = mouse_rows["event_args"].apply(
        lambda x: json.loads(x)[1]
    )
    mouse_rows.loc[mouse_rows["is_pressed"], "event_type"] = "MOUSE_DOWN"
    mouse_rows.loc[~mouse_rows["is_pressed"], "event_type"] = "MOUSE_UP"
    mouse_rows.loc[:, "event_args"] = mouse_rows["event_args"].apply(
        lambda x: "LMB" if json.loads(x)[0] == 1 else "RMB"
    )
    button_data.loc[mouse_mask] = mouse_rows

    # Assign frames
    button_data["frame"] = (button_data["timestamp"] / frame_duration).astype(int)

    # Simplify event types
    button_data.loc[
        button_data["event_type"].isin(["KEY_UP", "MOUSE_UP"]), "event_type"
    ] = "UP"
    button_data.loc[
        button_data["event_type"].isin(["KEY_DOWN", "MOUSE_DOWN"]), "event_type"
    ] = "DOWN"

    # Process events within each frame
    def process_frame_events(group):
        group = group.sort_values("timestamp")
        down_events = group[group["event_type"] == "DOWN"]
        up_events = group[group["event_type"] == "UP"]

        if len(down_events) > 0 and len(up_events) == 0:
            return down_events.iloc[-1:]
        elif len(up_events) > 0 and len(down_events) == 0:
            return up_events.iloc[-1:]

        last_event = group.iloc[-1]
        if last_event["event_type"] == "DOWN":
            return pd.DataFrame([last_event])
        else:
            if len(down_events) > 0:
                last_event = last_event.copy()
                last_event["event_type"] = "TAP"
                return pd.DataFrame([last_event])
            else:
                return pd.DataFrame([last_event])

    # Process events by frame and key
    button_data = (
        button_data.groupby(["frame", "event_args"])
        .apply(process_frame_events)
        .reset_index(drop=True)
    )

    # Convert to tensor
    total_frames = button_data["frame"].max() + 1
    button_tensor = torch.zeros((total_frames, len(KEYBINDS)), dtype=torch.bool)

    for _, row in button_data.iterrows():
        frame_idx = int(row["frame"])
        key_idx = KEYBINDS.index(row["event_args"])

        event_type = row["event_type"]
        if event_type == "DOWN":
            button_tensor[frame_idx:, key_idx] = True
        elif event_type == "UP":
            button_tensor[frame_idx:, key_idx] = False
        elif event_type == "TAP":
            button_tensor[frame_idx:, key_idx] = False
            button_tensor[frame_idx, key_idx] = True

    if return_tensor:
        return button_tensor

    # Split into chunks and save
    for chunk_idx in range(0, total_frames, SPLIT_SIZE):
        end_idx = min(chunk_idx + SPLIT_SIZE, total_frames)
        chunk = button_tensor[chunk_idx:end_idx]

        output_path = os.path.join(output_dir, f"{chunk_idx:08d}_buttons.pt")
        torch.save(chunk, output_path)


if __name__ == "__main__":
    for path in os.listdir(ROOT_DIR):
        print(f"Processing {path}")
        process_video(os.path.join(ROOT_DIR, path))
