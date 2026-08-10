# Attachments and media composer

Priority: P2

## What it should do

Support native file selection, drag/drop, clipboard images, screenshots, and removable attachment previews in addition to the existing `@` project-file mentions.

## How

Keep the contenteditable mention-chip model, but add a typed attachment part with filename, MIME type, size, local path, and upload/reference state. Use native file dialogs and a bounded staging directory; validate size/type in Rust and pass only safe references to `startTurn`.

## What it should look like

Show compact inline chips at the caret for files and images, preserving text order. Add drag-over highlighting, a paperclip button, thumbnail previews for images, upload progress, and clear/remove controls. Failed attachments should remain visible with a Retry action.
