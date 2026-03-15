import { Link, useLocation } from '@tanstack/react-router';
import { useAtomValue, useSetAtom } from 'jotai';
import {
  Podcast, AudioWaveform, SlidersHorizontal, SpellCheck,
  Mic, Sparkles, Shield, Clock, BarChart3, Wrench,
  ShieldAlert, UserPen, RefreshCw, Terminal,
} from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import {
  AppShell, Badge, Box, Divider, Group, Image, NativeSelect, ActionIcon,
  NavLink, ScrollArea, Stack, Text,
} from '@mantine/core';
import { StatusDot } from '../ui/StatusDot';
import {
  settingsAtom,
  dictationStateAtom,
  microphonesAtom,
  setupIssueLabelAtom,
  canOpenDebugTabAtom,
  stateLabels,
} from '../../stores/app';
import { addToast } from '../../stores/toasts';
import { invoke } from '../../lib/tauri';
import { reconcileMicSetting } from '../../lib/commands';
import type { MicDevice } from '../../types';

interface NavItem {
  to: string;
  label: string;
  icon: LucideIcon;
}

interface NavSection {
  label: string;
  items: NavItem[];
}

const navSections: NavSection[] = [
  {
    label: 'Dictation',
    items: [
      { to: '/settings/general', label: 'General', icon: SlidersHorizontal },
      { to: '/settings/audio', label: 'Audio', icon: AudioWaveform },
    ],
  },
  {
    label: 'Recognition',
    items: [
      { to: '/settings/models', label: 'Speech', icon: Mic },
      { to: '/settings/ai-enhancement', label: 'AI Enhancement', icon: Sparkles },
      { to: '/settings/content-classification', label: 'Classification', icon: ShieldAlert },
      { to: '/settings/personas', label: 'Personas', icon: UserPen },
      { to: '/settings/text-processing', label: 'Text Processing', icon: SpellCheck },
    ],
  },
  {
    label: 'System',
    items: [
      { to: '/settings/permissions', label: 'Permissions', icon: Shield },
      { to: '/settings/history', label: 'History', icon: Clock },
      { to: '/settings/advanced', label: 'Advanced', icon: Wrench },
    ],
  },
];

function isExactRoute(pathname: string, to: string) {
  const norm = pathname.replace(/\/+$/, '') || '/';
  const target = to.replace(/\/+$/, '') || '/';
  return norm === target;
}

export function SettingsNavbar() {
  const { pathname } = useLocation();
  const settings = useAtomValue(settingsAtom);
  const dictationState = useAtomValue(dictationStateAtom);
  const setupIssue = useAtomValue(setupIssueLabelAtom);
  const canOpenDebug = useAtomValue(canOpenDebugTabAtom);
  const microphones = useAtomValue(microphonesAtom);
  const setMicrophones = useSetAtom(microphonesAtom);
  const setSettings = useSetAtom(settingsAtom);

  const filteredSections = navSections.map((section) => ({
    ...section,
    items: section.items.filter((item) => {
      if (item.to === '/settings/ai-enhancement') {
        return settings?.beta_ai_enhancement_enabled;
      }
      if (item.to === '/settings/content-classification') {
        return settings?.beta_content_classification_enabled;
      }
      if (item.to === '/settings/personas') {
        return settings?.beta_personas_enabled;
      }
      return true;
    }),
  })).filter((section) => section.items.length > 0);

  async function refreshMicrophones() {
    try {
      const mics = await invoke<MicDevice[]>('list_microphones');
      setMicrophones(mics);
      if (settings) {
        const resolved = await reconcileMicSetting(settings, mics);
        setSettings(resolved);
      }
      addToast('success', 'Microphone list refreshed.');
    } catch (error) {
      addToast('error', `Failed refreshing microphones: ${String(error)}`);
    }
  }

  async function changeMicrophone(deviceId: string) {
    try {
      const updated = await invoke<typeof settings>('update_settings', {
        patch: { mic_device_id: deviceId === '' ? null : deviceId },
      });
      setSettings(updated);
      addToast('success', 'Microphone updated.');
    } catch (error) {
      addToast('error', `Failed to update settings: ${String(error)}`);
    }
  }

  const micOptions = [
    { value: '', label: 'System Default' },
    ...microphones.map((mic) => ({ value: mic.id, label: mic.name })),
  ];

  return (
    <>
      {/* ── Branding + Status ── */}
      <AppShell.Section px="md" pt={40} pb="sm" className="buttervoice-drag-region">
        <Stack align="center" gap={6}>
          <Image src="/logo.png" alt="ButterVoice" w="80%" h="auto" />
          <Group gap={6}>
            <StatusDot state={dictationState} />
            <Text size="xs" c="dimmed" fw={500}>{stateLabels[dictationState]}</Text>
          </Group>
          {setupIssue && (
            <Badge color="orange" variant="light" size="xs">
              Action: {setupIssue}
            </Badge>
          )}
        </Stack>
      </AppShell.Section>

      <Divider mx="sm" mb="xs" />

      {/* ── Navigation ── */}
      <AppShell.Section grow component={ScrollArea} p="xs">
        <NavLink
          component={Link}
          to="/dashboard"
          label="Studio"
          leftSection={<Podcast size={15} />}
          active={isExactRoute(pathname, '/dashboard')}
          variant="light"
          styles={{
            root: {
              borderRadius: 'var(--mantine-radius-sm)',
            },
          }}
        />
        <NavLink
          component={Link}
          to="/settings/stats"
          label="Stats"
          leftSection={<BarChart3 size={15} />}
          active={isExactRoute(pathname, '/settings/stats')}
          variant="light"
          styles={{
            root: {
              borderRadius: 'var(--mantine-radius-sm)',
              marginBottom: 'var(--mantine-spacing-sm)',
            },
          }}
        />

        {filteredSections.map((section, idx) => (
          <Box key={section.label} mb={4}>
            <Text
              size="xs"
              fw={700}
              c="dimmed"
              tt="uppercase"
              px="sm"
              pt={idx > 0 ? 'sm' : 4}
              pb={6}
              style={{ letterSpacing: '0.08em', fontSize: 10 }}
            >
              {section.label}
            </Text>

            {section.items.map((item) => {
              const Icon = item.icon;
              const active = isExactRoute(pathname, item.to);
              return (
                <NavLink
                  key={item.to}
                  component={Link}
                  to={item.to}
                  label={item.label}
                  leftSection={<Icon size={15} />}
                  active={active}
                  variant="light"
                  styles={{
                    root: {
                      borderRadius: 'var(--mantine-radius-sm)',
                      borderLeft: active
                        ? '2px solid var(--mantine-primary-color-filled)'
                        : '2px solid transparent',
                    },
                  }}
                />
              );
            })}
          </Box>
        ))}

        {canOpenDebug && (
          <>
            <Text
              size="xs"
              fw={700}
              c="dimmed"
              tt="uppercase"
              px="sm"
              pt="sm"
              pb={6}
              style={{ letterSpacing: '0.08em', fontSize: 10 }}
            >
              Developer
            </Text>
            <NavLink
              component={Link}
              to="/debug"
              label="Debug"
              leftSection={<Terminal size={15} />}
              active={isExactRoute(pathname, '/debug')}
              variant="light"
              styles={{
                root: {
                  borderRadius: 'var(--mantine-radius-sm)',
                  borderLeft: isExactRoute(pathname, '/debug')
                    ? '2px solid var(--mantine-primary-color-filled)'
                    : '2px solid transparent',
                },
              }}
            />
          </>
        )}
      </AppShell.Section>

      {/* ── Mic Selector Footer ── */}
      {settings && (
        <AppShell.Section px="sm" pb="sm">
          <Divider mb="sm" />
          <Stack gap={4}>
            <Text size="xs" fw={600} c="dimmed" px={4}>Microphone</Text>
            <Group gap={4} wrap="nowrap">
              <NativeSelect
                size="xs"
                data={micOptions}
                value={settings.mic_device_id ?? ''}
                onChange={(e) => changeMicrophone(e.currentTarget.value)}
                style={{ flex: 1 }}
              />
              <ActionIcon variant="subtle" size="sm" aria-label="Refresh microphones" onClick={refreshMicrophones}>
                <RefreshCw size={13} />
              </ActionIcon>
            </Group>
          </Stack>
        </AppShell.Section>
      )}
    </>
  );
}
