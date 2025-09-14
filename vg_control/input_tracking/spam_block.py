from typing import Callable


class SpamBlock:
    def __init__(self):
        self.buttons_pressed = {}

    def decorate(self, callback_fn: Callable[[int, bool], None]):
        """
        Wrap some callback_fn that takes in (int, bool)
        For callback_fn:
            - int: ID for thing that was pressed
            - bool: Whether this press was down (true) or a release (false)
        """

        def wrapper(button_id: int, is_down: bool):
            if is_down:
                if button_id not in self.buttons_pressed:
                    self.buttons_pressed[button_id] = True
                    callback_fn(button_id, is_down)
            else:
                if button_id in self.buttons_pressed:
                    del self.buttons_pressed[button_id]
                callback_fn(button_id, is_down)

        return wrapper
