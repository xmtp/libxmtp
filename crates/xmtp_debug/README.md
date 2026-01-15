# XMTP Debug

### Debug your app on local & dev XMTP environments

Supported Features:

- Generate Identities
- Generate Groups
- Generate Messages
- Inspect Generated Local Identities/Groups
- Export Generated Identities/Groups to JSON
- Invite external members to generated groups
- Three Supported log formats (Human, JSON, and logfmt)
  - log formats can be used for debugging, JSON & logfmt formats may be used
    with tools like [hl](https://github.com/pamburus/hl) or
    [lnav](https://lnav.org/)

### Intro

XMTP Debug is a comprehensive testing tool for the XMTP network. It may be used
to inspect

### Examples

---

#### Generate

##### Generate 1000 random identities

```
cargo xdbg generate --entity identity --amount 1000
```

##### Generate 100 random groups, inviting 50 random identities to each

```
cargo xdbg generate --entity group --amount 100 --invite 50
```

##### Generate 20 messages

```
cargo xdbg generate --entity message --amount 20
```

##### Generate 20 messages in a loop every 500 milliseconds

```
cargo xdbg generate --entity message --amount 20 --interval 500 --loop
```

##### Generate 20 messages in a loop every 500 milliseconds, raising maximum size of each message

```
cargo xdbg generate --entity message --amount 20 --interval 500 --loop --max-message-size 1000
```

#### Inspect

##### Inspect an InboxId

```
cargo xdbg inspect 1d8ec149b5670b1df0bbea0b9f2f0ba513eef805a02eafb37df3587fc23d89fe groups
```

#### Info

##### Show information about local generated state

```
cargo xdbg info
```

#### Export Identities to JSON

```
cargo xdbg export --entity identity | jq > identities.json
```

#### Query

##### Get information about identity updates for an inbox id

```
cargo xdbg query identity 01a43cdd27b196472687262ed5783006eabc7c26db9e09630bc5004b8fc689dc
```

##### Get information about key packages for multiple inboxes

```
cargo xdbg query fetch-key-packages d43e83f66ad7dbbe87add243806999d608bb0b6f7b88ba5efcaabdb532728309 01a43cdd27b196472687262ed5783006eabc7c26db9e09630bc5004b8fc689dc
```

##### Get information about the query log for multiple groups (optionally skipping unspecified commits)

```
cargo xdbg --backend dev query batch-query-commit-log e261da64fd225fc90034631945259cdf 0bc5493237d3399dddd3735a049ea237 --skip-unspecified
```

## Future Work

See [The Tracking Issue](https://github.com/xmtp/libxmtp/issues/1310) for
in-progress features & future work.
