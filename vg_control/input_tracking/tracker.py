import time
import asyncio

from .rawinputlib import (
    InputData,
    KeyboardData,
    MouseButtonData,
    MouseMoveData,
    MouseScrollData,
)
from .spam_block import SpamBlock
from ..constants import FPS, POLLS_PER_FRAME


# This object uses RawInputReader in a loop
# to continuously collect input
# note that we cut out timestamp as its preferable to just get timestamp in python
class InputTracker:
    def __init__(self):
        self.running = False
        self.callbacks = {}
        self.polling_delay = 1.0 / (FPS * POLLS_PER_FRAME)

        self.kb_spam_blocker = None
        self.mb_spam_blocker = None

        self.last_event_time = time.perf_counter()

    def set_callbacks(self, callbacks):
        self.callbacks = callbacks

    def get_time_since_last_event(self):
        """Returns the time in seconds since the last input event was received."""
        return time.perf_counter() - self.last_event_time

    def recv_input(self, input_data: InputData):
        if not self.running:
            return

        self.last_event_time = time.perf_counter()

        if isinstance(input_data, MouseMoveData):
            self.callbacks["mouse_move"](input_data.dx, input_data.dy)
        elif isinstance(input_data, MouseButtonData):
            self.callbacks["mouse_button"](input_data.button, input_data.down)
        elif isinstance(input_data, MouseScrollData):
            self.callbacks["mouse_scroll"](input_data.scrollAmount)
        elif isinstance(input_data, KeyboardData):
            self.callbacks["keyboard"](input_data.keyCode, input_data.down)

    async def __call__(self):
        self.running = True
        self.kb_spam_blocker = SpamBlock()
        self.mb_spam_blocker = SpamBlock()
        self.callbacks["keyboard"] = self.kb_spam_blocker.decorate(
            self.callbacks["keyboard"]
        )
        self.callbacks["mouse_button"] = self.mb_spam_blocker.decorate(
            self.callbacks["mouse_button"]
        )

    async def stop(self):
        self.kb_spam_blocker = None
        self.mb_spam_blocker = None
        self.running = False


if __name__ == "__main__":
    import asyncio

    async def main():
        tracker = InputTracker()

        def mouse_move_callback(dx, dy):
            timestamp = time.perf_counter()
            print(f"Mouse Move: timestamp={timestamp}, dx={dx}, dy={dy}")

        def mouse_button_callback(button, down):
            timestamp = time.perf_counter()
            print(f"Mouse Button: timestamp={timestamp}, button={button}, down={down}")

        def mouse_scroll_callback(scroll_amount):
            timestamp = time.perf_counter()
            print(f"Mouse Scroll: timestamp={timestamp}, scroll_amount={scroll_amount}")

        def keyboard_callback(key_code, down):
            timestamp = time.perf_counter()
            print(f"Keyboard: timestamp={timestamp}, key_code={key_code}, down={down}")

        callbacks = {
            "mouse_move": mouse_move_callback,
            "mouse_button": mouse_button_callback,
            "mouse_scroll": mouse_scroll_callback,
            "keyboard": keyboard_callback,
        }

        tracker.set_callbacks(callbacks)

        task = asyncio.create_task(tracker())

        try:
            await asyncio.sleep(10)  # Run for 10 seconds
        finally:
            tracker.stop()
            await task

    asyncio.run(main())
