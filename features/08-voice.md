# Voice

Priority: P2

## What it should do

Support push-to-talk and hands-free voice turns, with clear microphone permission, recording, transcription, cancellation, and playback states.

## How

Add a native audio bridge for microphone capture and playback. Stream or batch audio through the configured voice service, then submit the transcript through the same `startTurn` path as typed input. Store only the transcript by default and make recording cancellation safe.

## What it should look like

Add a microphone button beside Send. While recording, replace the composer controls with a waveform, elapsed time, Cancel, and Stop buttons. Show the resulting transcript as an editable draft before sending, and expose voice permissions in Settings.
