# UI conventions

`src/ui` is the design system. Its metrics were measured on an iOS 27 iPhone
17 Pro.

- Colors come from `useColors()` or a palette name on `color` props; never
  hard-code hex values in screens. Text uses `Text` variants (SwiftUI text
  styles with Dynamic Type); icons use `Symbol` (SF Symbols, Material Symbols
  on Android).
- Buttons are `GlassButton` and `GlassIconButton` (native SwiftUI glass on
  iOS). Grouped screens use `FormScreen` + `FormSection` + the rows in
  `FormRows.tsx`; make `FormScreen` the route's first view so the large
  title collapses. A form with text fields passes `avoidsKeyboard`, not
  `automaticallyAdjustKeyboardInsets`: React Native clamps that offset
  without the bar's inset, so any keyboard change moves a resting form
  under the bar.
- `Stack.Toolbar.Button` icons go through `useToolbarIcon()`, because Android
  needs an image. A titled toolbar button (Next, Create) is
  `ToolbarTextButton`: Android draws no titled `Stack.Toolbar.Button`.
  Pass `confirmation` for a confirming action (semibold Save, Add). A
  toolbar that also holds icons uses `toolbarTextItem()`, because
  `Stack.Toolbar` reads only its direct `Stack.Toolbar.*` children on iOS.
  `NavigationSubtitle` sets the iOS navigation subtitle.
- List rows with a long-press menu use `ContextMenu` (a UIKit context menu
  in `modules/messenger-platform`; Android renders the children only).
  Confirmations that SwiftUI shows as popovers (`.confirmationDialog` on a
  list) use `PopoverConfirmation`. SwiftUI `.alert`s use `AlertDialog`, so
  Cancel and default buttons take the app tint; React Native's
  `showAlert` (UIAlertController) draws them in the label color.
- A single-line `TextInput` in body text sets
  `lineBreakModeIOS={singleLineBreakMode}` (`TextFieldRow` does): Fabric
  reuses native text input views, and a reused multiline view leaves a long
  placeholder clipped at a word with no ellipsis. A multiline `TextInput`
  reads its placeholder as its label while empty: set `accessibilityLabel`
  only when it has text, or VoiceOver reads the label twice.
- Text that wraps uses `WrappedText` (SwiftUI's wrapped line height once
  it wraps) or `BubbleText` in message bubbles. Inside a List or Form keep
  the default; elsewhere (scroll views, cards) pass `leading="standard"`.
- The Debug gallery shows every component and sample screens:
  `xcrun simctl openurl <udid> "xmtp-messenger-rn:///dev/gallery?section=components"`
  (sections: `components`, `settings`, `details`, `chats`, `chat`, `connect`).

## Chat

The chat stores are pure TypeScript in `src/core` (`chatTimeline.ts`, `chatComposer.ts`, `messageActions.ts`, with `chatHistory.ts` and `chatMessages.ts`) and are tested against the in-memory conversation in `chatFakes.ts`. `src/features/chat/chatRuntime.ts` wires them to the SDK (`sdkChatQueries.ts`), the send journal and the stream hub; views call its actions (`startReply`, `cancelReply`, `retry`, `edit`, `react`, `requestDelete`, `deleteMessage`, `copyText`, `submit`). A new content kind adds a `ChatContent` case, its SDK mapping in `chatContent` and a renderer in `MessageContent.tsx`. `MessageBubbleWrapper.tsx` delegates to `features/chat/interactions` (SwiftUI context menu with the quick-reaction preview, swipe to reply, VoiceOver actions, the send flight and the "Delete message?" alert); their rules are in `src/core/messageInteractions.ts`. `ComposerSlots.tsx` (attach menu, staged attachments) is a slot for later features. Settings renders `<SendReviewSection session={session} />` (`src/features/settings/sendReviewSlot.tsx`) directly in its `FormScreen`; the section brings its own `FormSection` and reloads on focus. Inside the context menu the bubble is hosted by SwiftUI without a width proposal, so `MessageInteractions` bounds it with the row's maximum bubble width. The SwiftUI host also takes no touches outside its frame, so on a row with reaction badges the hosted view grows 12 pt up to cover the badges' overhang. The quick-reaction buttons in the menu preview do not receive taps (the preview is one accessibility element); use the React item.

The Reactions sheet (`features/reactions`) and the emoji picker (`features/emoji`, catalog in `src/core/emojiData.ts`) draw their own headers. react-native-screens lays out a form sheet whose content root has a scroll view as a direct child as a scrolling sheet (warning "FormSheet with ScrollView expects at most 2 subviews"), and the list draws over the header: give the screen one `<View collapsable={false}>` root.

The request bar takes its Accept, Delete request and Block decisions from the same `RequestDecisionsStore` as Message requests (`useRequestDecisions`). Accept leaves Chats below the open chat and keeps it mounted.

## New message

`features/new-conversation` is a page sheet with its own stack: step 1 (`index`) and the review (`review`) share one `NewConversationStore` (`src/core/newConversation.ts`, recipient resolution and its tests) that `NewConversationSheet` owns in the sheet's layout (`useNewConversation()`). Create closes the sheet and pushes the new chat. A create already sent cannot be taken back: while it runs the review disables Back and Change people, and the sheet cannot be dismissed (no swipe down, no Android back), so the new chat opens when it ends. `useScrollEdge()` (`scrollEdge.ts`) gives a form in a sheet SwiftUI's top edge: none at rest, the hard edge once content is under the bar. A static `scrollEdgeEffects: { top: "hard" }` draws the edge at rest too.

## Details

`src/core/details.ts` (`DetailsStore`: loads, ordered group writes, pending members and one poller) and `src/core/metadata.ts` (field rules and `MetadataStore`) are pure stores tested in `node --test`. `src/features/details/detailsStores.ts` gives Details, Edit group and Add member one shared store per conversation (`useSharedDetailsStore`), so errors and pending members show on Details after a sheet closes; Member, Permissions and Disappearing each own a store (`useOwnDetailsStore`). A sheet at a partial detent draws glass behind translucent cards: `useSheetFormProps()` (`sheetDetent.tsx`) returns the `FormScreen` props for the sheet's current detent. Block, Leave group and Remove from group confirm with `PopoverConfirmation` over the list.

## Attachments

The upload drafts and downloads are one pure store, `src/core/attachmentStore.ts` (with `attachments.ts` for models, copy, failure mapping and the saved drafts, and `sharedMedia.ts`), tested against `attachmentFakes.ts`. The recovery screen's chat choices are a separate pure store, `src/core/recoveryChats.ts`. `src/features/attachments` wires it to the SDK (`sdkAttachmentGateway.ts`) and the send journal: a send uploads first, then queues `sendRemoteAttachment` with the draft ID as the journal ID. Each open chat has one store (`chatAttachments.ts`), shared by the composer slots and the file cards in `MessageContent.tsx`. The stores of a session share the saved drafts and the running sends, and a screen that shows drafts watches them (`watchDrafts`): a send that another screen of the chat finishes or stops shows at once. Picked files are copied into `<profile>/temp/attachment-staging/<uuid>` within the server limit and removed after `attachments.create`; Open and Save copy a verified file to `<profile>/exports/<uuid>/`, which the session removes when it ends. Image previews use a memory-only cache, so decrypted files stay in protected storage. Attachment recovery (`/attachment-recovery`, opened from Settings › Developer tools and from a staged file's "Review send") and Media and files (`/shared-media/<conversation>`, opened from Details) live in the same folder.

- `app/dev/attachment-probe.tsx` (Debug only): `xmtp-messenger-rn:///dev/attachment-probe?action=<action>&n=<new value>`. Actions: `limit` (`&bytes=<n>`: the upload limit a chat reads when it opens, for the "File exceeds the server upload limit" check), `clearLimit`, `holdUploads` (uploads wait, so the app can be stopped mid-upload) and `releaseUploads`.
- Launch with `XMTP_ALLOW_PRIVATE_ATTACHMENTS=1` for the local object store, and with `XMTP_UI_TEST_ATTACHMENT=<name>:<base64>` (Debug, at most 64 KiB) for the attach menu's "Test file".
