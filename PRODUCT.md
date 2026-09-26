# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

The owner uses a Windows PC and wants to manually browse through several country-specific, persistent sessions without switching the whole PC's VPN.

## Product Purpose

RegionBox manages independent browser workspaces, each with its own browser profile and VPN tunnel.

## Operating Context

A Tauri Windows desktop app hosts the web interface. Docker Desktop runs Linux containers locally. The user confirmed an existing paid NordVPN subscription, multiple Chromium sessions, and United States, Germany, and United Kingdom for initial testing.

## Capabilities and Constraints

The first version must actually start, stop, and display browsers concurrently through different countries. Browser data must survive restarts. VPN loss must block direct internet access. VPN service credentials are entered inside the app. Onboarding checks and installs Windows/Docker prerequisites, downloads images, and guides the first connection. Windows approval, restart, BIOS configuration, and Docker's own terms remain user actions. Mobile is future work.

## Brand Commitments

Name: RegionBox. The user repeatedly requested the simplest working version and explicitly required standard shadcn components with no custom UI system. Keep the interface compact, familiar, and focused on workspace operation.

## Evidence on Hand

docs/rough_idea_what_i_want_to_build.md and the user's confirmed choices. Network behavior remains unverified until tested with working VPN credentials.

## Product Principles

- Show actual runtime state.
- Save browser data independently for every workspace.
- Keep workspaces independent when starting and stopping.
- Make failures understandable and recoverable.
