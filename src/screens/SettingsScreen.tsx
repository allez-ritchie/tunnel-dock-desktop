import { useState, type ReactNode } from "react";
import { BrandMark, ConfirmDialog, Icon, SecureNotice } from "../components/ui";
import { BundleCard } from "./BundleCard";
import { strings, type ThemeMode } from "../strings";

interface Props {
  theme: ThemeMode;
  knownHostsCount: number;
  autostart: boolean;
  version: string;
  tunnelCount: number;
  appCount: number;
  groupCount: number;
  onThemeChange: (theme: ThemeMode) => void;
  onClearKnownHosts: () => Promise<void>;
  onMessage: (text: string) => void;
  /** 导入配置包后刷新配置与计数。 */
  onImported: () => Promise<void>;
}

export function SettingsScreen({
  theme,
  knownHostsCount,
  autostart,
  version,
  tunnelCount,
  appCount,
  groupCount,
  onThemeChange,
  onClearKnownHosts,
  onMessage,
  onImported,
}: Props) {
  const [confirmClear, setConfirmClear] = useState(false);
  const [clearing, setClearing] = useState(false);

  async function clearHosts() {
    setClearing(true);
    try {
      await onClearKnownHosts();
      onMessage(strings.settingsClearedHosts);
    } finally {
      setClearing(false);
      setConfirmClear(false);
    }
  }

  return (
    <div className="screen tab-screen settings-screen">
      <header className="page-header">
        <div className="brand">
          <BrandMark size={44} />
          <div>
            <span className="brand-kicker">TUNNELDOCK</span>
            <h1>{strings.settingsTitle}</h1>
            <p className="brand-tagline">{strings.appTagline}</p>
          </div>
        </div>
      </header>

      <div className="settings-scroll">
        <SettingsCard
          icon={<Icon name="shield" size={20} />}
          title={strings.settingsBackground}
          subtitle={strings.settingsBackgroundDesc}
        >
          <div className="info-row">
            <span className="info-dot" />
            <div>
              <strong>{strings.settingsTray}</strong>
              <p>{strings.settingsTrayBody}</p>
            </div>
          </div>
          <div className="info-row">
            <span className={`info-dot ${autostart ? "is-on" : ""}`} />
            <div>
              <strong>{strings.settingsAutostart}</strong>
              <p>
                {autostart
                  ? strings.settingsAutostartOn
                  : strings.settingsAutostartOff}
              </p>
              <p className="subtle">{strings.settingsAutostartHint}</p>
            </div>
          </div>
        </SettingsCard>

        <SettingsCard
          icon={<Icon name="apps" size={20} />}
          title={strings.settingsData}
          subtitle={strings.settingsDataDesc}
        >
          <p className="settings-counts mono">
            {strings.settingsDataCounts(tunnelCount, appCount, groupCount)}
          </p>
          <p className="field-hint">{strings.settingsDataHint}</p>
        </SettingsCard>

        <BundleCard
          tunnelCount={tunnelCount}
          appCount={appCount}
          groupCount={groupCount}
          onMessage={onMessage}
          onImported={onImported}
        />

        <SettingsCard
          icon={<Icon name="sun" size={20} />}
          title={strings.settingsAppearance}
          subtitle={strings.settingsAppearanceDesc}
        >
          <div className="segmented theme-segmented">
            {(
              [
                { value: "system", label: strings.settingsThemeSystem, icon: "monitor" },
                { value: "dark", label: strings.settingsThemeDark, icon: "moon" },
                { value: "light", label: strings.settingsThemeLight, icon: "sun" },
              ] as const
            ).map((option) => (
              <button
                key={option.value}
                type="button"
                className={theme === option.value ? "is-active" : ""}
                onClick={() => onThemeChange(option.value)}
              >
                <Icon name={option.icon} size={15} />
                {option.label}
              </button>
            ))}
          </div>
        </SettingsCard>

        <SettingsCard
          icon={<Icon name="key" size={20} />}
          title={strings.settingsHostKeys}
          subtitle={strings.settingsHostKeysCount(knownHostsCount)}
        >
          <p className="field-hint">{strings.settingsHostKeysDesc}</p>
          <button
            type="button"
            className="btn outline sm danger-outline"
            disabled={knownHostsCount === 0 || clearing}
            onClick={() => setConfirmClear(true)}
          >
            <Icon name="trash" size={16} />
            {strings.settingsClearHosts}
          </button>
        </SettingsCard>

        <SettingsCard
          icon={<Icon name="lock" size={20} />}
          title={strings.settingsSecurity}
          subtitle={strings.settingsSecurityBody}
        >
          <SecureNotice text={strings.editorSecretNotice} />
        </SettingsCard>

        <SettingsCard
          icon={<Icon name="info" size={20} />}
          title={strings.settingsAbout}
          subtitle={strings.settingsVersion(version)}
        >
          <div className="about-row">
            <BrandMark size={44} />
            <div>
              <strong>{strings.appName}</strong>
              <p>{strings.appTagline}</p>
            </div>
          </div>
          <p className="field-hint">{strings.settingsAboutBody}</p>
          <p className="field-hint subtle">{strings.settingsOss}</p>
        </SettingsCard>
      </div>

      {confirmClear ? (
        <ConfirmDialog
          title={strings.settingsClearHostsTitle}
          body={strings.settingsClearHostsBody}
          confirmLabel={strings.settingsClearHosts}
          onCancel={() => setConfirmClear(false)}
          onConfirm={() => void clearHosts()}
        />
      ) : null}
    </div>
  );
}

function SettingsCard({
  icon,
  title,
  subtitle,
  children,
}: {
  icon: ReactNode;
  title: string;
  subtitle: string;
  children: ReactNode;
}) {
  return (
    <section className="settings-card">
      <header>
        <span className="settings-icon">{icon}</span>
        <div>
          <h2>{title}</h2>
          <p>{subtitle}</p>
        </div>
      </header>
      <div className="settings-card-body">{children}</div>
    </section>
  );
}
