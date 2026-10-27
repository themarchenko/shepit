import { useState, type ReactNode } from "react";
import { useSettings, type SettingsStore } from "../shared/useSettings";
import AboutPane from "./AboutPane";
import AppearancePane from "./AppearancePane";
import DictionaryPane from "./DictionaryPane";
import GeneralPane from "./GeneralPane";
import PolishingPane from "./PolishingPane";

type Tab = "general" | "appearance" | "polishing" | "dictionary" | "about";

// Inline SVG: the apps ship no icon font.
function Icon({ children }: { children: ReactNode }) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      {children}
    </svg>
  );
}

const TABS: { id: Tab; label: string; icon: ReactNode }[] = [
  {
    id: "general",
    label: "General",
    icon: (
      <Icon>
        <circle cx="12" cy="12" r="3" />
        <path d="M12 2.8v2.6M12 18.6v2.6M2.8 12h2.6M18.6 12h2.6M5.5 5.5l1.9 1.9M16.6 16.6l1.9 1.9M18.5 5.5l-1.9 1.9M7.4 16.6l-1.9 1.9" />
      </Icon>
    ),
  },
  {
    id: "appearance",
    label: "Appearance",
    icon: (
      <Icon>
        <path d="M12 3.5a8.5 8.5 0 1 0 0 17c1.4 0 2-.9 2-1.8 0-1.4-1.2-1.6-1.2-2.9 0-1 .8-1.8 1.9-1.8H17a3.5 3.5 0 0 0 3.5-3.5C20.5 6.7 16.7 3.5 12 3.5z" />
        <circle cx="7.8" cy="11" r="1" />
        <circle cx="10.5" cy="7.6" r="1" />
        <circle cx="14.8" cy="7.6" r="1" />
      </Icon>
    ),
  },
  {
    id: "polishing",
    label: "Polishing",
    icon: (
      <Icon>
        <path d="M12 3.5l1.6 4.4 4.4 1.6-4.4 1.6L12 15.5l-1.6-4.4L6 9.5l4.4-1.6z" />
        <path d="M18.5 15l.7 1.8 1.8.7-1.8.7-.7 1.8-.7-1.8-1.8-.7 1.8-.7z" />
      </Icon>
    ),
  },
  {
    id: "dictionary",
    label: "Dictionary",
    icon: (
      <Icon>
        <path d="M5 19V5.5A2 2 0 0 1 7 3.5h12v14H7a2 2 0 0 0-2 2" />
        <path d="M7 17.5h12v3H7a2 2 0 0 1-2-1.5" />
        <path d="M9 7.5h6" />
      </Icon>
    ),
  },
  {
    id: "about",
    label: "About",
    icon: (
      <Icon>
        <circle cx="12" cy="12" r="8.5" />
        <path d="M12 11v5.2" />
        <circle cx="12" cy="7.9" r="0.6" fill="currentColor" />
      </Icon>
    ),
  },
];

function Pane({ tab, store }: { tab: Tab; store: SettingsStore }) {
  if (!store.settings) return null;
  const props = { settings: store.settings, update: store.update };
  if (tab === "appearance") return <AppearancePane {...props} />;
  if (tab === "polishing") return <PolishingPane {...props} />;
  if (tab === "dictionary") return <DictionaryPane {...props} />;
  if (tab === "about") return <AboutPane {...props} />;
  return <GeneralPane {...props} />;
}

export default function SettingsApp() {
  const store = useSettings();
  // Opens on General: Windows has no What's new card, which is why macOS opens on Appearance.
  const [tab, setTab] = useState<Tab>("general");

  return (
    <div className="sp-settings">
      <div className="sp-tabbar" role="tablist">
        {TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            role="tab"
            className="sp-tab"
            data-tab={t.id}
            aria-selected={t.id === tab}
            onClick={() => setTab(t.id)}
          >
            {t.icon}
            {t.label}
          </button>
        ))}
      </div>
      {/* Keyed by tab so the pane's enter animation plays on every switch. */}
      <div key={tab} className="sp-pane" role="tabpanel">
        <Pane tab={tab} store={store} />
      </div>
    </div>
  );
}
