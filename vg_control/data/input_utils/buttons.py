"""
Button presses
"""

import json
import os
import numpy as np
import pandas as pd

from ...input_tracking.keybinds import CODE_TO_KEY
from ...constants import FPS


def get_ascii(keycode_int):
    return CODE_TO_KEY.get(keycode_int, f"Unknown key: {keycode_int}")


def get_keycode(ascii_char):
    for code, key in CODE_TO_KEY.items():
        if key == ascii_char:
            return code
    return None


def get_button_stats(csv_path):
    """
    Get stats on button presses including:
    - WASD actions per minute
    - Number of unique buttons pressed
    - Button diversity metric
    - Total number of keyboard events
    """
    # Get WASD keycodes
    wasd_codes = [
        get_keycode("W"),
        get_keycode("A"),
        get_keycode("S"),
        get_keycode("D"),
    ]

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
        duration_minutes = end_time / 60
    else:
        duration_minutes = button_data["timestamp"].max() / 60

    # Filter for keyboard events only
    keyboard_data = button_data[button_data["event_type"] == "KEYBOARD"].reset_index(
        drop=True
    )

    # Extract keycodes and press states
    keyboard_data["keycode"] = keyboard_data["event_args"].apply(
        lambda x: json.loads(x)[0]
    )
    keyboard_data["is_pressed"] = keyboard_data["event_args"].apply(
        lambda x: json.loads(x)[1]
    )

    # Calculate stats
    wasd_presses = keyboard_data[
        (keyboard_data["keycode"].isin(wasd_codes)) & (keyboard_data["is_pressed"])
    ].shape[0]
    wasd_apm = wasd_presses / duration_minutes

    unique_keys = keyboard_data[keyboard_data["is_pressed"]]["keycode"].nunique()

    # Calculate button diversity using normalized entropy
    key_counts = keyboard_data[keyboard_data["is_pressed"]]["keycode"].value_counts()
    probs = key_counts / key_counts.sum()
    entropy = -(probs * np.log2(probs)).sum()
    max_entropy = np.log2(len(key_counts))
    diversity = entropy / max_entropy if max_entropy > 0 else 0

    # Get total keyboard events
    total_keyboard_events = len(keyboard_data)

    return {
        "wasd_apm": wasd_apm,
        "unique_keys": unique_keys,
        "button_diversity": diversity,
        "total_keyboard_events": total_keyboard_events,
    }
