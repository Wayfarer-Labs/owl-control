import pandas as pd
import time
import asyncio
import os

from .tracker import InputTracker

"""
Quick Rundown on Event Datasets:

When stored as CSVs, each row has:
- timestamp [unix time]
- event type (see events.py) [str]
- event_args (see callback args) [list[any]]

Event Args on different kinds of events:
KB: [keycode : int, key_down : bool] (key down = true, key up = false)
MB: [button_idx : int, key_down : bool]
MM: [dx : int, dy : int]
SCROLL: [amt : int] (positive = up)

"""

header = ["timestamp", "event_type", "event_args"]


class DataWriter:
    """
    DataWriter is used to write inputs to a csv file with timestamps using InputTracker.
    """

    def __init__(self):
        self.writing = False
        self.tracker = InputTracker()
        self.current_data = []

    async def start(self):
        """
        Starts recording user inputs. Starts a loop that adds events onto self.current_data
        """
        if self.writing:
            return

        self.writing = True
        self.start_over = False

        def keyboard_callback(keycode, state):
            self.current_data.append(
                {
                    "timestamp": time.perf_counter(),
                    "event_type": "KEYBOARD",
                    "event_args": [keycode, state],
                }
            )

        def mouse_button_callback(button, state):
            self.current_data.append(
                {
                    "timestamp": time.perf_counter(),
                    "event_type": "MOUSE_BUTTON",
                    "event_args": [button, state],
                }
            )

        def mouse_move_callback(dx, dy):
            self.current_data.append(
                {
                    "timestamp": time.perf_counter(),
                    "event_type": "MOUSE_MOVE",
                    "event_args": [dx, dy],
                }
            )

        def scroll_callback(scroll_amt):
            self.current_data.append(
                {
                    "timestamp": time.perf_counter(),
                    "event_type": "SCROLL",
                    "event_args": [scroll_amt],
                }
            )

        callbacks = {
            "keyboard": keyboard_callback,
            "mouse_button": mouse_button_callback,
            "mouse_move": mouse_move_callback,
            "mouse_scroll": scroll_callback,
        }

        self.tracker.set_callbacks(callbacks)
        tracker_task = asyncio.create_task(self.tracker())

        try:
            while self.writing:
                await asyncio.sleep(0.01)

        finally:
            await self.tracker.stop()
            await tracker_task

    # Logging start and end from OBS recording directly
    def start_fn(self, time):
        self.current_data.append(
            {"timestamp": time, "event_type": "START", "event_args": []}
        )

    def end_fn(self, time):
        self.current_data.append(
            {"timestamp": time, "event_type": "END", "event_args": []}
        )

    async def end(self, file_path):
        self.writing = False
        while self.tracker.running:  # Don't continue until tracker is done adding stuff
            await asyncio.sleep(0.01)
        await asyncio.sleep(0.5)

        original_length = len(self.current_data)

        # Validate and clean data
        clean_data = [
            {col: row.get(col) for col in header}
            for row in self.current_data
            if all(key in row for key in header)
        ]

        if len(clean_data) != original_length:
            print(
                f"Warning: Dropped {original_length - len(clean_data)} malformed rows"
            )

        df = pd.DataFrame(clean_data, columns=header)
        if "/" in file_path:
            os.makedirs(os.path.dirname(file_path), exist_ok=True)
        df.to_csv(file_path, index=False)
        self.current_data = []


class DataWriterClient:
    """
    Wrapper object for DataWriter to simplify usage as much as possible

    await start with path to start recording
    await end to save that recording and start anew
    """

    def __init__(self):
        self.writer = DataWriter()
        self.tracker_task = None
        self.write_path = None

    async def start(self, path):
        """
        Start recording in a given path (to csv file)
        """
        self.write_path = path
        self.tracker_task = asyncio.create_task(self.writer.start())

    async def end(self):
        """
        End recording
        """
        if self.write_path is None or self.tracker_task is None:
            return

        await self.writer.end(self.write_path)

        # Only wait for tracker task if we're not currently in it
        if asyncio.current_task() != self.tracker_task:
            await self.tracker_task

        self.write_path = None
        self.tracker_task = None


if __name__ == "__main__":

    async def main():
        client = DataWriterClient()
        await client.start("data_dump/data.csv")
        print("Checkpoint 1")
        await asyncio.sleep(5)
        print("Checkpoint 2")
        await client.end()

        await client.start("data_dump/data_2.csv")
        print("Checkpoint 3")
        await asyncio.sleep(2)
        print("Checkpoint 4")
        await client.end()

    asyncio.run(main())
