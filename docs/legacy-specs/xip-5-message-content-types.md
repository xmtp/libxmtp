---
xip: 5
title: Message content types
description: Interoperable framework for supporting different types of content
author: Martin Kobetic (@mkobetic), Nick Molnar (@neekolas)
status: Final
type: Standards Track
category: Interface
created: 2022-02-08
---

## Abstract

This XIP introduces a framework for interoperable support of different types of content in XMTP messages. At the heart of it are provisions for attaching meta-information to the content that will identify its type and structure, and allow for its correct decoding from the encoded form used for transport inside XMTP messages. Additionally it defines provisions for optional content compression.

The XIP envisions community based, iterative development of a library of content types over time. Content type identifiers are scoped to allow different entities to define their own. The proposed framework provides an interface for registering content type codecs with the client for transparent encoding and decoding of content.

This XIP is not intended to define content types themselves, those should be proposed through separate XRCs. The only content type defined here is a simple plain text type (`xmtp.org/text`).

## Motivation

The SDK API currently accepts only `string` as the message content, which suggests to the user that only plain text is supported. We want to foster a community around the protocol that would be motivated to build a wide array of very different clients and applications around it. We need to enable at least future possibility of using different types of content beyond just plain text (rich text, images, video, sound, file attachments, etc).

Given that this is a large and complex topic we don't want to have to solve it all right now. We want a flexible framework that will allow building a rich library of various types of supported content over time. This library should be open to collaboration with other organizations. The framework should be simple, but powerful enough to not hinder future development. The framework should also provide a reasonably friendly API that isn't too onerous to use. This XIP forms an explicit foundation for future XRCs proposing new types of content to be carried by the protocol.

To support future evolution, both the type identifier itself and the types need a way to version their definitions. Specific types may require additional parameters that apply to those types only, these parameters should carry metadata necessary for correct decoding of the content.

It is expected that many clients will want the ability to carry multiple different types of content in the same message. To keep the basic framework simple, the expectation is to handle such payload in dedicated structured content types that will be defined in the future.

Since the set of known content types will be changing over time, clients will need to be ready to handle situations where they cannot correctly decode or present content that arrives in a message. There should be a way to provide an optional `fallback` in the basic framework that can be used to provide description of the content that couldn't be presented.

## Specification

### Protocol

At the network level the Message payload is currently represented by Ciphertext

```protobuf
message Ciphertext {
    message AES256GCM_HKDFSHA256 {
        bytes hkdfSalt = 1;
        bytes gcmNonce = 2;
        bytes payload = 3;
    }
    oneof union {
        AES256GCM_HKDFSHA256 aes256GcmHkdfSha256 = 1;
    }
}
```

There is no reason to expose the content type meta information unencrypted, so it makes sense to define a new type that will be embedded in the Ciphertext.payload bytes. That means there will be two layers of protobuf encoding. Let's refer to the outer encoding layer that turns the entire message into bytes as `Message Encoding` and the inner layer that turns the payload into bytes as `Content Encoding`. The content itself needs to be encoded into bytes as well in a manner that is dictated by the content type. For text content that would usually involve employing a standard character encoding, like UTF-8.

```protobuf
enum Compression {
  deflate = 0;
  gzip = 1;
}

message EncodedContent {
  ContentTypeId type = 1;
  map<string, string> parameters = 2;
  optional string fallback = 3;
  optional Compression compression = 5;
  bytes content = 4;
}
```

The full encoding process will go through the following steps:

1. Encode the content into its binary form
2. Optionally compress the binary content
3. Wrap it into the EncodedContent structure and encode it using protobuf (content encoding)
4. Encrypt the EncodedContent bytes and wrap those in the Ciphertext structure
5. Wrap the Ciphertext in the Message structure and encode it using protobuf (message encoding)

The encoded Message is then further wrapped in transport protocol envelopes. The decoding process is the reverse of the above steps.

We will not introduce a separate version for the embedded type, it can be tied to the version of the overall protocol.

#### Content Type Identifier and Parameters

`ContentTypeId` identifies the type and format of the information contained in the content. It needs to carry enough information to be able to route the decoding process to the correct decoding machinery. As such the identifier should carry the following bits of information:

* authority ID
* content type ID
* content type version

Identifier format is tied to the protocol version. Changes to the format will require corresponding protocol version adjustment. Such changes MAY add new information to the identifier but it SHOULD only be information that is required to match the identifier with the corresponding decoding machinery or information that is required for all content types (like the content type version). Any information that is specific to a content type SHOULD be carried in the content type parameters field. Here is the definition of the identifier type:

```protobuf
message ContentTypeId {
  string authorityId = 1;  // authority governing this content type
  string typeId = 2;  // type identifier
  uint32 versionMajor = 3; // major version of the type
  uint32 versionMinor = 4; // minor version of the type
}
```

Any content type MUST have well defined parameters (or clearly state that no parameters are required/allowed), and any implementation MUST support all valid parameters for the content type.

Authority ID identifies the entity that governs a suite of content types, their definitions and implementations. Authority ID SHOULD be unique and be widely recognized as belonging to the entity. DNS domains or ENS names can serve this purpose (e.g. `uniswap.eth`). The authority is responsible for providing a definition of the content type and its encoding parameters as well as the associated implementation.

`xmtp.org` is one such entity and the `xmtp.org` authority ID value is reserved for standard content types only. Standard content types are those that have been adopted through the XIP process.

Type ID identifies particular type of content that can be handled by a specific implementation of its encoding/decoding rules. Content type version allows future evolution of the content type definition.

Type version is captured in the common major.minor form intended to convey the associated semantics that versions differing in the minor version only MUST be backward compatible, i.e. a client supporting an earlier version MUST be able to adequately present content with later version. Content type authority MUST manage the evolution of content type in a manner that respects this constraint.

For human communication only we are also proposing an alternative representation of an ID in the form of a string with the following structure `authority/type[:major.minor]`. For the plain text type identifier proposed below that translates to `xmtp.org/text` or `xmtp.org/text:1.0`. This representation MUST NOT be used at the protocol level, only the binary representation is allowed. Clients MUST NOT be expected to parse this string.

Due forethought should be given when choosing identifiers as there are no provisions to change them once they have been in use. A new identifier introduces a new (assumed unrelated) authority or content type as far as the protocol is concerned.

### API

To accommodate the new content type framework, the low level `Message` encode/decode API has to work in terms of bytes instead of strings. The protocol level `EncodedContent` message introduced above will be represented by an interface that allows bundling the content bytes with the content type metadata.

```ts
export type ContentTypeId = {
  authorityId: string
  typeId: string
  versionMajor: number
  versionMinor: number
}

export interface EncodedContent {
  type: ContentTypeId
  parameters: Record<string, string>
  fallback?: string
  compression?: Compression
  content: Uint8Array
}
```

This is a fairly simple change but makes for a very crude and hard to use API. Given that content types should be highly reusable it makes sense to provide a framework that will facilitate this reuse and provide some common content types out of the box. The framework should provide automatic content encoding/decoding based on the type of the provided content.

Supported content types must be submitted to the message sending API with a content type identifier. Each content type will have an associated `ContentCodec<T>`. The Client will maintain a registry of supported codecs, that will be used to look up codecs based on the `ContentTypeId` associated with the content.

```ts
export interface CodecRegistry {
  codecFor(contentType: ContentTypeId): ContentCodec<any> | undefined
}

export interface ContentCodec<T> {
  contentType: ContentTypeId
  encode(content: T, registry: CodecRegistry): EncodedContent
  decode(content: EncodedContent, registry: CodecRegistry): T
}
```

The `contentType` field of the codec is used to match the codec with the corresponding type of content. The major version declared by the codec's `contentType` is the maximum version supported by the codec. The codec SHOULD support all the previous versions up to its declared maximum version as long as those versions are in use.

New content types and content type versions SHOULD be proposed through XRCs. The XRC should

* define the content type identifier and version
* define any parameters and their semantics
* include a reference implementation of the codec

The codecs SHOULD comply with all the provisions of this XIP. Any deviations MUST be specified in their corresponding XRC.

### xmtp.org/text

This XIP defines a content type for plain text content. The content type id is `ContentTypeText`, the codec is `TextCodec`. The content value for this content type must be a simple `string`. This is the default content type, which is assumed when a `send` call does not have the `contentType` option.

```ts
export const ContentTypeText = {
  authorityId: 'xmtp.org',
  typeId: 'text',
  versionMajor: 1,
  versionMinor: 0,
}

export class TextCodec implements ContentCodec<string> {
  get contentType(): string {
    return ContentTypeText
  }

  ...
}
```

This content type has an optional `encoding` parameter indicating the character encoding used to encode the string into bytes. The default encoding is `UTF-8`. Only `UTF-8` is currently supported, additional encodings can be added through an XRC.

The codec implementation is part of the XMTP SDK.

### Implementation

The mapping between content types and their codecs will be managed at the Client level. The Client maintains a registry of supported types and codecs initialized to a default set of codecs that can be overridden/extended through `CreateOptions`.

```ts
export default class Client {
  ...
  registerCodec(codec: ContentCodec<any>): void
  ...
  codecFor(contentType: ContentTypeId): ContentCodec<any> | undefined 
  ...
```

The `Message` type will be augmented to hold the decoded `content` and `contentType` instead of just the `decrypted: string`.

```ts
export default class Message implements proto.Message {
  header: proto.Message_Header // eslint-disable-line camelcase
  ciphertext: Ciphertext
  decrypted?: Uint8Array
  contentType?: ContentTypeId
  content?: any
  error?: Error
  ...

```

Note that the `Message.text` getter that previously just returned the `decrypted` string, will have to be replaced with `Message.content`. The clients of the API will need to interrogate `Message.contentType` and do the right thing.

If an unrecognized content type is received the `Message.error` will be set accordingly. If `contentFallback` is present `Message.content` will be set to that. In order to be able to reliably distinguish the actual content from the fallback, we will introduce a special `ContentTypeId`.

```ts
export const ContentTypeFallback = {
  authorityId: 'xmtp.org',
  typeId: 'fallback',
  versionMajor: 1,
  versionMinor: 0,
}
```

## Rationale

There are decades of prior art in this area. Probably the most familiar example of content type identification scheme are filename extensions. The relevant lesson here is that simple string is likely insufficient for carrying the necessary parameters required to correctly decode the contents (what encoding is the `.txt` file using?). While structured file formats can easily embed those parameters in the file itself, if we do want to support unstructured payload, e.g. plain text, we probably should have a way to attach parameters to the content type identifier itself.

MIME framework (the underlying standard of email, http and other widely used protocols) has a fairly involved system using several headers Content-Type, Content-Transfer-Encoding, Content-Disposition etc. Notably the Content-Type header allows embedding arbitrary set of parameters in the header value along with the primary type identifier (media type). Most relevant is [RFC 2046](https://datatracker.ietf.org/doc/html/rfc2046) which discusses the basic media types. At the highest level it recognizes 5 fundamental media types: text, image, audio, video and composite media types. The composite media type is of particular interest as it allows combining different media types in single payload. As soon as you support composite media type, there are additional aspects that likely need to be addressed, e.g. are the different parts just different renderings of the same information (multipart/alternative), or are they different parts of longer narrative (multipart/mixed). Is any individual part meant to be rendered inline with the rest of the content, or is it meant to be an attachment that can be open/saved separately (Content-Disposition)? MIME also prescribes the central authority (IANA) that manages the registry of all recognized media types and their required or optional parameters.

## Backwards Compatibility

Since the new EncodedType message is embedded in the Ciphertext.payload bytes, this change doesn't break the protocol, strictly speaking, however any newer client would struggle interpretting the payload as EncodedContent unless it conforms. So this is a breaking change in that sense. Any older messages will be broken once the new protocol is deployed.

At the API level the changes are even more pronounced, since the input and output of the API is now potentially any type instead of just `string`. Extracting the content from the `Message` now requires interrogating the resulting value to determine which type of content it is and handling it accordingly. Clients SHOULD also take an appropriate action when encountering content type they do not recognize, and use the `contentFallback` when available. Clients SHOULD register codecs only for those types that they are prepared to handle.

## Reference Implementation

[https://github.com/xmtp/xmtp-js/pull/68](https://github.com/xmtp/xmtp-js/pull/68)

## Security Considerations

This API change allows transmitting arbitrary and therefore potentially dangerous types of content. Complex decoding or presentation logic can trigger undesirable or dangerous behavior in the receiving client. The authority of any given content type SHOULD provide suitable guidance on how to handle the content type safely.

## Copyright

Copyright and related rights waived via [CC0](https://creativecommons.org/publicdomain/zero/1.0/).
