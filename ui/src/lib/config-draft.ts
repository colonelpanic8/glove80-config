// The persistent-config draft, lifted to App scope so the Persistent config
// tab and the Toggles tab edit the same document (toggle persist/boot bits
// are blob fields; the switchboard needs the record list).

import { useEffect, useState } from "react";

import {
  decodeLightingConfig,
  encodeLightingConfig,
  type ConfigRecord,
  type LightingConfig,
} from "./host-protocol";

const DRAFT_STORAGE_KEY = "glove80-lightbench-config-draft-v1";

export const EMPTY_CONFIG: LightingConfig = {
  togglePersistMask: 0,
  toggleInitialState: 0,
  records: [],
};

export function cloneConfig(config: LightingConfig): LightingConfig {
  return {
    togglePersistMask: config.togglePersistMask,
    toggleInitialState: config.toggleInitialState,
    records: config.records.map(cloneRecord),
  };
}

export function cloneRecord(record: ConfigRecord): ConfigRecord {
  return {
    activation: { ...record.activation },
    cells: record.cells.map((cell) => ({ key: cell.key, effect: { ...cell.effect } })),
  };
}

/** Restore the saved draft; anything that does not survive a full encode
 * round-trip (the codec's own validation) is discarded. */
function loadDraft(): LightingConfig {
  try {
    const raw = localStorage.getItem(DRAFT_STORAGE_KEY);
    if (!raw) return cloneConfig(EMPTY_CONFIG);
    return decodeLightingConfig(encodeLightingConfig(JSON.parse(raw) as LightingConfig));
  } catch {
    return cloneConfig(EMPTY_CONFIG);
  }
}

export interface ConfigDraft {
  config: LightingConfig;
  setConfig: React.Dispatch<React.SetStateAction<LightingConfig>>;
  /** Selected record index in the Persistent config tab. */
  selected: number;
  setSelected: React.Dispatch<React.SetStateAction<number>>;
}

export function useConfigDraft(): ConfigDraft {
  const [config, setConfig] = useState(loadDraft);
  const [selected, setSelected] = useState(0);

  useEffect(() => {
    try {
      localStorage.setItem(DRAFT_STORAGE_KEY, JSON.stringify(config));
    } catch {
      // Draft persistence is best-effort; never block editing on storage.
    }
  }, [config]);

  return { config, setConfig, selected, setSelected };
}

/** Toggle ids referenced by any record of the draft, ascending. */
export function referencedToggleIds(config: LightingConfig): number[] {
  const ids = new Set<number>();
  for (const record of config.records) {
    if (record.activation.kind === "toggle") ids.add(record.activation.id);
  }
  return [...ids].sort((a, b) => a - b);
}
