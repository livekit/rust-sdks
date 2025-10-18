import sys
import os

sys.path.append(os.path.abspath(os.path.join(os.path.dirname(__file__), '..', 'generated', 'python')))

from livekit_uniffi import *

def main():
    credentials = ApiCredentials(key="devkey", secret="secret")

    jwt = generate_token(
        options=TokenOptions(room_name="test", identity="some_participant"),
        credentials=credentials,
    )
    print(f"Generated JWT: {jwt}")

    decoded_grants = verify_token(
        token=jwt,
        credentials=credentials,
    )
    print(f"Verified generated JWT: {decoded_grants}")


if __name__ == "__main__":
    main()
