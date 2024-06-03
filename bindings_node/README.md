# Node bindings for the MLS client

> **Important**  
> These bindings are currently in **Alpha** status. Do not use in production as the API is not final and certain functionality may not work as intended.

## Useful commands

- `yarn`: Installs all dependencies (required before building)
- `yarn build:release`: Build a release version of the Node bindings for the current platform

## Testing

There are several test scripts written in Node located in the `/scripts` folder.

Test users are available as exports in `users.mjs`. To register all users on the network, run the `setup.mjs` script.

Before running any of the test scripts, a local XMTP node must be running. This can be achieved by running `./dev/up` at the root of this repository.
