"""
Gamepad inputs
"""

import json
import numpy as np
import pandas as pd


def get_gamepad_stats(csv_path):
    """
    Get stats on gamepad inputs including:
    - Button presses per minute
    - Number of unique buttons pressed
    - Button diversity metric
    - Total number of gamepad events
    - Axis movement statistics
    - Button value changes (analog buttons like triggers)
    """
    # Load and preprocess the CSV data
    gamepad_data = pd.read_csv(csv_path)

    # Find start time and normalize timestamps
    start_time = gamepad_data.head(1000)[
        gamepad_data.head(1000)["event_type"] == "START"
    ].iloc[-1]["timestamp"]

    gamepad_data = gamepad_data[gamepad_data["timestamp"] >= start_time].reset_index(
        drop=True
    )
    gamepad_data["timestamp"] -= start_time

    # Trim to end event if exists
    end_rows = gamepad_data[gamepad_data["event_type"] == "END"]
    if not end_rows.empty:
        end_time = end_rows.iloc[0]["timestamp"]
        gamepad_data = gamepad_data[gamepad_data["timestamp"] <= end_time].reset_index(
            drop=True
        )
        duration_minutes = end_time / 60
    else:
        duration_minutes = gamepad_data["timestamp"].max() / 60

    # Filter for gamepad events only
    gamepad_events = gamepad_data[
        gamepad_data["event_type"].isin(
            ["GAMEPAD_BUTTON", "GAMEPAD_BUTTON_VALUE", "GAMEPAD_AXIS"]
        )
    ].reset_index(drop=True)

    button_apm = 0.0
    unique_buttons = 0
    diversity = 0.0
    total_button_events = 0
    axis_activity = 0.0
    max_axis_movement = 0.0

    # Parse event arguments
    gamepad_events["event_args"] = gamepad_events["event_args"].apply(json.loads)

    # Separate different types of gamepad events
    button_events = gamepad_events[
        gamepad_events["event_type"] == "GAMEPAD_BUTTON"
    ].copy()
    button_value_events = gamepad_events[
        gamepad_events["event_type"] == "GAMEPAD_BUTTON_VALUE"
    ].copy()
    axis_events = gamepad_events[gamepad_events["event_type"] == "GAMEPAD_AXIS"].copy()

    # Process button events
    if not button_events.empty:
        button_events["button_idx"] = button_events["event_args"].apply(lambda x: x[0])
        button_events["is_pressed"] = button_events["event_args"].apply(lambda x: x[1])

        # Calculate button press statistics
        button_presses = button_events[button_events["is_pressed"]].shape[0]
        button_apm = button_presses / duration_minutes if duration_minutes > 0 else 0

        unique_buttons = button_events[button_events["is_pressed"]][
            "button_idx"
        ].nunique()

        # Calculate button diversity using normalized entropy
        button_counts = button_events[button_events["is_pressed"]][
            "button_idx"
        ].value_counts()
        if not button_counts.empty:
            probs = button_counts / button_counts.sum()
            entropy = -(probs * np.log2(probs)).sum()
            max_entropy = np.log2(len(button_counts))
            diversity = entropy / max_entropy if max_entropy > 0 else 0
        else:
            diversity = 0.0

        total_button_events = len(button_events)

    # Process axis events
    if not axis_events.empty:
        axis_events["axis_idx"] = axis_events["event_args"].apply(lambda x: x[0])
        axis_events["axis_value"] = axis_events["event_args"].apply(lambda x: x[1])

        # Calculate axis movement statistics
        axis_activity = axis_events["axis_value"].abs().mean()
        max_axis_movement = axis_events["axis_value"].abs().max()

    # Process button value events (analog buttons like triggers)
    if not button_value_events.empty:
        # Add button value events to total button events
        total_button_events += len(button_value_events)

    return {
        "button_apm": button_apm,
        "unique_buttons": unique_buttons,
        "button_diversity": diversity,
        "total_button_events": total_button_events,
        "axis_activity": axis_activity,
        "max_axis_movement": max_axis_movement,
        "total_gamepad_events": len(gamepad_events),
    }
