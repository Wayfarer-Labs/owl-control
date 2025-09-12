"""
Constants used throughout the application
"""

# Recording settings
FPS = 60  # Frames per second for tracking
MIN_FOOTAGE = 30  # This many seconds needed before something is "worth saving"
MAX_FOOTAGE = 60 * 10  # How long should each individual clip be?
RECORDING_WIDTH = 640  # Video recording width
RECORDING_HEIGHT = 360  # Video recording height

# API endpoints
API_BASE_URL = "https://api.openworldlabs.ai"
UPLOAD_ENDPOINT = "/v1/upload"

# File formats
DATA_FILE_FORMAT = "vgc"  # OWL Control data format
ROOT_DIR = (
    "./data_dump/games/"  # User should be able to set this, but we will need to use it
)
POLLS_PER_FRAME = 20  # Hard coded
