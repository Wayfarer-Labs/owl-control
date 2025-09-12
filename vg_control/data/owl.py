from dotenv import load_dotenv
import os

load_dotenv()

import tarfile
import shutil
import json
from datetime import datetime

from ..constants import (
    ROOT_DIR,
    MIN_FOOTAGE,
    MAX_FOOTAGE,
    RECORDING_WIDTH,
    RECORDING_HEIGHT,
    FPS,
)

from .input_utils.buttons import get_button_stats
from .input_utils.mouse import get_mouse_stats
from .uploader import upload_archive

# Directory structure might be nested, but the root dirs will always have a .mp4 and .csv


def filter_invalid_sample(vid_path, csv_path, meta_path) -> list[str]:
    """
    Detect invalid videos.

    Return value is a list of reasons for invalidity. If empty, the sample is valid.
    """
    with open(meta_path) as f:
        metadata = json.load(f)
    duration = float(metadata["duration"])

    invalid_reasons = []

    if duration < MIN_FOOTAGE:
        invalid_reasons.append(f"Video length {duration:.2f} too short.")
    if duration > MAX_FOOTAGE + 10:
        invalid_reasons.append(f"Video length {duration:.2f} too long.")

    bitrate = 2  # mbps
    # Get video file size in MB
    vid_size = os.path.getsize(vid_path) / (1024 * 1024)
    vid_size *= 8  # megabits
    expected_bits = bitrate * duration

    if (
        vid_size < 0.25 * expected_bits
    ):  # Less than quarter of expected size is unlikely
        invalid_reasons.append(
            f"Video size {vid_size:.2f} Mb too small compared to expected {expected_bits:.2f} Mb"
        )

    btn_stats = get_button_stats(csv_path)
    mouse_stats = get_mouse_stats(csv_path)

    # Filter out samples with too little activity
    if (
        btn_stats["wasd_apm"] < 10
    ):  # Less than 20 actions per minute is likely AFK/inactive
        invalid_reasons.append(
            f"WASD actions per minute too low: {btn_stats['wasd_apm']:.1f}"
        )

    if btn_stats["total_keyboard_events"] < 50:  # Too few keyboard events overall
        invalid_reasons.append(
            f"Too few keyboard events: {btn_stats['total_keyboard_events']}"
        )

    # Filter out samples with abnormal mouse behavior
    if mouse_stats["overall_max"] < 0.05:  # Very little mouse movement
        invalid_reasons.append(
            f"Mouse movement too small: {mouse_stats['overall_max']:.3f}"
        )

    if mouse_stats["overall_max"] > 10_000:  # Unreasonably large mouse movements
        invalid_reasons.append(
            f"Mouse movement too large: {mouse_stats['overall_max']:.1f}"
        )

    # Add stats to metadata
    extra_metadata = {
        "input_stats": {
            "wasd_apm": btn_stats["wasd_apm"],
            "unique_keys": btn_stats["unique_keys"],
            "button_diversity": btn_stats["button_diversity"],
            "total_keyboard_events": btn_stats["total_keyboard_events"],
            "mouse_movement_std": mouse_stats["overall_std"],
            "mouse_x_std": mouse_stats["x_std"],
            "mouse_y_std": mouse_stats["y_std"],
            "mouse_max_movement": mouse_stats["overall_max"],
            "mouse_max_x": mouse_stats["max_x"],
            "mouse_max_y": mouse_stats["max_y"],
        }
    }

    if "input_stats" not in metadata:
        metadata.update(extra_metadata)
        with open(meta_path, "w") as f:
            json.dump(metadata, f, indent=4)

    return invalid_reasons


class OWLDataManager:
    def __init__(self, token, progress_mode=False):
        self.staged_files = []
        self.staging_dir = "staging"
        self.current_tar_uuid = None
        self.token = token
        self.progress_mode = progress_mode
        self.total_duration = 0.0  # Track total duration of uploaded videos
        self.total_bytes = 0  # Track total bytes of files
        self.staged_bytes = 0  # Track bytes staged so far
        os.makedirs(self.staging_dir, exist_ok=True)

    def process_individual_sessions(self, verbose=False):
        """Process each session as an individual tar file and upload immediately."""
        sessions_processed = 0

        for root, dirs, files in os.walk(ROOT_DIR):
            if ".uploaded" in files or ".invalid" in files:
                continue

            has_mp4 = any([fname.endswith(".mp4") for fname in files])
            has_csv = any([fname.endswith(".csv") for fname in files])
            has_metadata = any([fname == "metadata.json" for fname in files])

            if has_mp4 and has_csv and has_metadata:
                mp4_file = next(f for f in files if f.endswith(".mp4"))
                csv_file = next(f for f in files if f.endswith(".csv"))

                mp4_path = os.path.join(root, mp4_file)
                csv_path = os.path.join(root, csv_file)
                meta_path = os.path.join(root, "metadata.json")

                # Check validity
                invalid_reasons = []
                try:
                    invalid_reasons = filter_invalid_sample(
                        mp4_path, csv_path, meta_path
                    )
                except Exception as e:
                    invalid_reasons.append(f"Error checking validity: {e}")

                if len(invalid_reasons) > 0:
                    invalid_path = os.path.join(root, ".invalid")

                    if not verbose:
                        print(
                            f"Failed to process {os.path.abspath(mp4_path)}; see {os.path.abspath(invalid_path)} for details"
                        )
                    else:
                        print(f"Failed to process {os.path.abspath(mp4_path)}:")
                        for reason in invalid_reasons:
                            print(f"  - {reason}")

                    with open(invalid_path, "w") as f:
                        for reason in invalid_reasons:
                            f.write(reason + "\n")

                    continue

                # Read duration from metadata and track bytes
                metadata_dict = {}
                try:
                    with open(meta_path) as f:
                        metadata_dict = json.load(f)
                    duration = float(metadata_dict.get("duration", 0))
                    self.total_duration += duration
                except Exception as e:
                    print(f"Warning: Could not read duration from {meta_path}: {e}")

                # Track file sizes for statistics
                mp4_size = os.path.getsize(mp4_path)
                csv_size = os.path.getsize(csv_path)
                meta_size = os.path.getsize(meta_path)
                self.total_bytes += mp4_size + csv_size + meta_size

                # Create tar for this single session
                import uuid

                tar_name = f"{uuid.uuid4().hex[:16]}.tar"

                with tarfile.open(tar_name, "w") as tar:
                    tar.add(mp4_path, arcname=mp4_file)
                    tar.add(csv_path, arcname=csv_file)
                    tar.add(meta_path, arcname="metadata.json")

                # Upload immediately with metadata
                try:
                    upload_archive(
                        self.token,
                        tar_name,
                        progress_mode=self.progress_mode,
                        video_filename=mp4_file,
                        control_filename=csv_file,
                        video_duration_seconds=metadata_dict.get("duration")
                        if metadata_dict
                        else None,
                        video_width=RECORDING_WIDTH,
                        video_height=RECORDING_HEIGHT,
                        video_fps=FPS,
                        # video_codec not set here since it depends on user's OBS settings
                    )
                    with open(os.path.join(root, ".uploaded"), "w") as f:
                        f.write("")
                    self.staged_files.append(root)
                    sessions_processed += 1
                finally:
                    if os.path.exists(tar_name):
                        os.remove(tar_name)

        return sessions_processed > 0

    def clear_upload_status(self):
        for root, dirs, files in os.walk(ROOT_DIR):
            if ".uploaded" in files:
                os.remove(os.path.join(root, ".uploaded"))


def upload_all_files(token, progress_mode=False):
    manager = OWLDataManager(token, progress_mode=progress_mode)
    has_files = manager.process_individual_sessions()

    # Output final stats for the main process to capture
    if progress_mode:
        final_stats = {
            "phase": "complete",
            "total_files_uploaded": len(manager.staged_files),
            "total_duration_uploaded": manager.total_duration,
            "total_bytes_uploaded": manager.total_bytes,
        }
        print(f"FINAL_STATS: {json.dumps(final_stats)}")

    return {
        "files_uploaded": len(manager.staged_files) if has_files else 0,
        "total_duration": manager.total_duration,
    }


if __name__ == "__main__":
    import sys

    if len(sys.argv) < 2:
        print("Error: Token argument is required")
        sys.exit(1)

    token = sys.argv[1]
    try:
        upload_all_files(token)
        print("Upload completed successfully")
    except Exception as e:
        print(f"Error during upload: {str(e)}")
        sys.exit(1)
