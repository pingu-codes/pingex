# Scheduled tasks

Priority: P1

## What it should do

Create recurring Codex tasks, run them immediately, pause/resume them, inspect previous runs, and archive or delete a task without losing its history.

## How

Add a native scheduler service with durable task definitions, timezone-aware schedules, run records, retry state, and notifications. Route each run through the existing thread/turn pipeline with an explicit project, model, permission, and integration scope. Make scheduler state observable as events rather than polling the whole sidebar.

## What it should look like

Add a Scheduled tasks page with a searchable list and a detail pane. Each task shows prompt, schedule, next run, status, source integrations, and recent runs. Provide `New task`, `Run now`, `Pause`, `Resume`, `Edit`, and `Delete` actions with clear failure states.
