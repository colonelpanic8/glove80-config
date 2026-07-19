// Toggle switchboard: every lighting toggle as a first-class object.
//
// A toggle here is one card: a host-side NAME (browser-local; the config
// blob has no room for names), the LIVE state on the keyboard (GET_TOGGLE on
// load, SET_TOGGLE on flip), the two blob fields (boot state, persist
// opt-in) and the records the toggle activates. Toggles appear when a draft
// record references them or when explicitly added; ids are 0–31.

import { useEffect, useMemo, useState } from "react";

import type { ConfigDraft } from "../lib/config-draft";
import { referencedToggleIds } from "../lib/config-draft";
import { CONFIG_TOGGLE_COUNT, FEATURE_TOGGLES, type Capabilities } from "../lib/host-protocol";
import { StatusError, type ProtocolClient } from "../lib/protocol-client";
import { nextFreeToggleId, type ToggleMeta } from "../lib/toggle-meta";
import type { StatusUpdate } from "./OverlayPanel";

/** What the keyboard said about one toggle id. */
type DeviceToggleState =
  | { kind: "loading" }
  | { kind: "known"; on: boolean }
  /** UNKNOWN_TOGGLE: the active on-keyboard config references no such id. */
  | { kind: "unconfigured" }
  | { kind: "error"; message: string };

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

interface TogglePanelProps {
  client: ProtocolClient | null;
  capabilities: Capabilities | null;
  draft: ConfigDraft;
  meta: ToggleMeta;
  onMetaChange: (update: (meta: ToggleMeta) => ToggleMeta) => void;
  onJumpToRecord: (index: number) => void;
  onStatus: (status: StatusUpdate) => void;
}

export function TogglePanel({
  client,
  capabilities,
  draft,
  meta,
  onMetaChange,
  onJumpToRecord,
  onStatus,
}: TogglePanelProps) {
  const { config, setConfig } = draft;
  const supportsToggles = !capabilities || (capabilities.featureBits & FEATURE_TOGGLES) !== 0;
  const [deviceStates, setDeviceStates] = useState(new Map<number, DeviceToggleState>());

  const referenced = useMemo(() => referencedToggleIds(config), [config]);

  // Probe GET_TOGGLE for all 32 ids on connect. The protocol has no
  // enumerate-toggles command, but UNKNOWN_TOGGLE cleanly separates ids the
  // active on-keyboard config defines from ids it does not — so toggles
  // configured on the keyboard appear here even before the draft references
  // them. The client serializes requests, so this is one-at-a-time.
  useEffect(() => {
    if (!client || !supportsToggles) {
      setDeviceStates(new Map());
      return;
    }
    let cancelled = false;
    setDeviceStates(new Map());
    void (async () => {
      for (let id = 0; id < CONFIG_TOGGLE_COUNT; id++) {
        let state: DeviceToggleState;
        try {
          state = { kind: "known", on: await client.getToggle(id) };
        } catch (error) {
          state =
            error instanceof StatusError && error.status === "unknownToggle"
              ? { kind: "unconfigured" }
              : { kind: "error", message: errorMessage(error) };
        }
        if (cancelled) return;
        setDeviceStates((current) => new Map(current).set(id, state));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [client, supportsToggles]);

  const visibleIds = useMemo(() => {
    const named = Object.keys(meta.names).map(Number);
    const deviceKnown = [...deviceStates.entries()]
      .filter(([, state]) => state.kind === "known")
      .map(([id]) => id);
    return [...new Set([...referenced, ...meta.addedIds, ...named, ...deviceKnown])].sort((a, b) => a - b);
  }, [referenced, meta, deviceStates]);

  const flip = async (id: number, on: boolean) => {
    if (!client) return;
    setDeviceStates((current) => new Map(current).set(id, { kind: "loading" }));
    try {
      const applied = await client.setToggle(id, on);
      setDeviceStates((current) => new Map(current).set(id, { kind: "known", on: applied }));
      const name = meta.names[id];
      onStatus({
        tone: "ok",
        message: `${name ? `“${name}” (toggle ${id})` : `Toggle ${id}`} is now ${applied ? "on" : "off"} on the keyboard`,
      });
    } catch (error) {
      if (error instanceof StatusError && error.status === "unknownToggle") {
        setDeviceStates((current) => new Map(current).set(id, { kind: "unconfigured" }));
        onStatus({
          tone: "warn",
          message: `Toggle ${id} is not in the keyboard's active config — apply a config that references it first`,
        });
      } else {
        setDeviceStates((current) => new Map(current).set(id, { kind: "error", message: errorMessage(error) }));
        onStatus({ tone: "error", message: errorMessage(error) });
      }
    }
  };

  const setBit = (field: "togglePersistMask" | "toggleInitialState", id: number, on: boolean) => {
    setConfig((current) => ({
      ...current,
      [field]: on ? current[field] | (1 << id) : current[field] & ~(1 << id),
    }));
  };

  const addToggle = () => {
    const id = nextFreeToggleId(visibleIds);
    if (id === null) {
      onStatus({ tone: "warn", message: `All ${CONFIG_TOGGLE_COUNT} toggle ids are in use` });
      return;
    }
    onMetaChange((current) => ({ ...current, addedIds: [...current.addedIds, id] }));
    onStatus({ tone: "ok", message: `Added toggle ${id} — name it, then give it a record to light something` });
  };

  const removeToggle = (id: number) => {
    onMetaChange((current) => {
      const names = { ...current.names };
      delete names[id];
      return { names, addedIds: current.addedIds.filter((existing) => existing !== id) };
    });
  };

  const setName = (id: number, name: string) => {
    onMetaChange((current) => {
      const names = { ...current.names };
      if (name.trim() === "") delete names[id];
      else names[id] = name;
      // Naming a toggle keeps it visible even with no records yet.
      const addedIds = current.addedIds.includes(id) ? current.addedIds : [...current.addedIds, id];
      return { names, addedIds };
    });
  };

  const recordsFor = (id: number) =>
    config.records
      .map((record, index) => ({ record, index }))
      .filter(({ record }) => record.activation.kind === "toggle" && record.activation.id === id);

  const liveState = (id: number): DeviceToggleState | null => (client ? (deviceStates.get(id) ?? null) : null);

  return (
    <section className="workspace">
      <aside className="tool-panel">
        <section>
          <div className="section-heading">
            <span className="step-number">01</span>
            <div>
              <h2>Switchboard</h2>
              <p>Named switches over toggle overlays</p>
            </div>
          </div>
          <p className="panel-prose">
            A toggle (id 0–31) switches its lighting records on and off, independent of the active
            layer. Flipping a switch here talks to the keyboard immediately; what it lights is
            defined by the toggle's records in the persistent config.
          </p>
          <button className="button tool wide" onClick={addToggle} disabled={visibleIds.length >= CONFIG_TOGGLE_COUNT}>
            + Add toggle{nextFreeToggleId(visibleIds) !== null ? ` (id ${nextFreeToggleId(visibleIds)})` : ""}
          </button>
        </section>

        <section>
          <div className="section-heading compact">
            <span className="step-number">02</span>
            <div>
              <h2>Names</h2>
              <p>Stored in this browser, not on the keyboard</p>
            </div>
          </div>
          <p className="panel-prose">
            The config blob has no field for names, so they live in this browser's storage (per
            keyboard) and in a small <code>.names.json</code> sidecar written next to the
            <code> .bin</code> on export. Import the sidecar to bring names back.
          </p>
        </section>

        <section className="scene-tools">
          <div className="section-heading compact">
            <span className="step-number">03</span>
            <div>
              <h2>Boot & persistence</h2>
              <p>Blob fields, applied with the config</p>
            </div>
          </div>
          <p className="panel-prose">
            “On at boot” and “persist state” are fields of the persistent config
            (<code>toggle_initial_state</code>, <code>toggle_persist_mask</code>). Editing them
            changes the draft — apply it from the <strong>Persistent config</strong> tab to store
            them on the keyboard.
          </p>
        </section>
      </aside>

      <section className="keyboard-stage toggle-stage" aria-label="Toggle switchboard">
        {visibleIds.length === 0 ? (
          <div className="toggle-empty">
            <h2>No toggles yet</h2>
            <p>
              Add a toggle here, or create a “+ Toggle” record in the Persistent config tab — every
              toggle referenced by a record shows up automatically.
            </p>
          </div>
        ) : (
          <ul className="switchboard">
            {visibleIds.map((id) => {
              const state = liveState(id);
              const records = recordsFor(id);
              const persisted = (config.togglePersistMask & (1 << id)) !== 0;
              const bootOn = (config.toggleInitialState & (1 << id)) !== 0;
              return (
                <li key={id} className="switch-card">
                  <div className="switch-card-head">
                    <label className={`switch ${state?.kind === "known" ? "" : "unknown"}`}>
                      <input
                        type="checkbox"
                        checked={state?.kind === "known" ? state.on : false}
                        disabled={!client || !supportsToggles || state === null || state.kind === "loading" || state.kind === "unconfigured"}
                        onChange={(event) => void flip(id, event.target.checked)}
                        aria-label={`Toggle ${id} live state`}
                      />
                      <span className="switch-slider" aria-hidden="true" />
                    </label>
                    <input
                      className="toggle-name"
                      value={meta.names[id] ?? ""}
                      placeholder={`Toggle ${id}`}
                      spellCheck={false}
                      maxLength={48}
                      onChange={(event) => setName(id, event.target.value)}
                      aria-label={`Name for toggle ${id} (stored in this browser only)`}
                      title="Host-side name — stored in this browser and the export sidecar, never on the keyboard"
                    />
                    <span className="toggle-id">#{id}</span>
                    {records.length === 0 && state?.kind !== "known" && (
                      <button
                        className="switch-remove"
                        onClick={() => removeToggle(id)}
                        title="Remove this toggle from the switchboard (no records reference it)"
                        aria-label={`Remove toggle ${id}`}
                      >
                        ✕
                      </button>
                    )}
                  </div>

                  <div className="switch-live">
                    {!client ? (
                      <span className="live-hint">Connect a keyboard to flip it live</span>
                    ) : !supportsToggles ? (
                      <span className="live-hint">This keyboard does not advertise toggles</span>
                    ) : state === null || state.kind === "loading" ? (
                      <span className="live-hint">Asking the keyboard…</span>
                    ) : state.kind === "known" ? (
                      <span className={`live-state ${state.on ? "on" : ""}`}>{state.on ? "ON" : "OFF"} on the keyboard</span>
                    ) : state.kind === "unconfigured" ? (
                      <span className="live-hint warn">
                        Not in the keyboard's active config — apply a config referencing it to flip it
                      </span>
                    ) : (
                      <span className="live-hint warn">{state.message}</span>
                    )}
                  </div>

                  <div className="switch-blob-fields">
                    <label title="toggle_initial_state bit: the state this toggle boots with (when not persisted)">
                      <input type="checkbox" checked={bootOn} onChange={(event) => setBit("toggleInitialState", id, event.target.checked)} />
                      on at boot
                    </label>
                    <label title="toggle_persist_mask bit: opt in to persisting the runtime state across reboots">
                      <input type="checkbox" checked={persisted} onChange={(event) => setBit("togglePersistMask", id, event.target.checked)} />
                      persist state
                    </label>
                    {persisted && <span className="blob-note">runtime state survives reboots; boot state is ignored</span>}
                  </div>

                  <div className="switch-records">
                    {records.length === 0 ? (
                      <span className="live-hint">
                        {state?.kind === "known"
                          ? "No records in the local draft — use “Load from keyboard” in Persistent config to edit what it lights."
                          : "No records — this toggle lights nothing yet. Add a “+ Toggle” record in Persistent config."}
                      </span>
                    ) : (
                      records.map(({ record, index }) => (
                        <button
                          key={index}
                          className="record-chip"
                          onClick={() => onJumpToRecord(index)}
                          title="Open this record in the Persistent config tab"
                        >
                          Record {index + 1} · {record.cells.length} {record.cells.length === 1 ? "cell" : "cells"}
                        </button>
                      ))
                    )}
                  </div>
                </li>
              );
            })}
          </ul>
        )}
        <p className="board-caption">
          Live state changes are RAM-only unless a toggle opts into persistence; boot/persist bits
          only reach the keyboard when the config draft is applied.
        </p>
      </section>
    </section>
  );
}
