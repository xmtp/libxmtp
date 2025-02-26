# XMTP Debug

## WARNING

Nothing about xdbg is secure. it openly writes private keys as plaintext in json
and in the database. No attempt is made at forward or backwards compatibility.
Client message databases are secured by an empty string. This tool is meant
purely as a debugging tool, and not for personal or production use.

Use at your own risk.

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
./xdbg generate --entity identity --amount 1000
```

##### Generate 100 random groups, inviting 50 random identities to each

```
./xdbg generate --entity group --amount 100 --invite 50
```

##### Generate 20 messages

```
./xdbg generate --entity message --amount 20
```

##### Generate 20 messages in a loop every 500 milliseconds

```
./xdbg generate --entity message --amount 20 --interval 500 --loop
```

##### Generate 20 messages in a loop every 500 milliseconds, raising maximum size of each message

```
./xdbg generate --entity message --amount 20 --interval 500 --loop --max-message-size 1000
```

#### Inspect

##### Inspect an InboxId

```
./xdbg inspect 1d8ec149b5670b1df0bbea0b9f2f0ba513eef805a02eafb37df3587fc23d89fe groups
```

#### Info

##### Show information about local generated state

```
./xdbg info
```

#### Export Identities to JSON

```
./xdbg export --entity identity | jq > identities.json
```

## Future Work

See [The Tracking Issue](https://github.com/xmtp/libxmtp/issues/1310) for
in-progress features & future work.
