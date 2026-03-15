import { useAtomValue, useSetAtom } from 'jotai';
import { Check } from 'lucide-react';
import {
  Box, Button, Center, Group, Kbd, Paper, Progress, Stack, Stepper, Text, ThemeIcon, Title,
} from '@mantine/core';
import {
  settingsAtom,
  permissionsAtom,
  onboardingStepAtom,
  onboardingDismissedAtom,
  recommendedModelAtom,
  restartPromptVisibleAtom,
  restartingAppAtom,
} from '../../stores/app';
import { addToast, clearToasts } from '../../stores/toasts';
import { invoke } from '../../lib/tauri';
import type { PermissionKind, PermissionsStatus } from '../../types';
import { hotkeyDisplayLabel } from '../../types';
import { usePermissionRefreshOnFocus } from '../../lib/hooks/usePermissionRefreshOnFocus';

const permissionJourney: { kind: PermissionKind; label: string; detail: string; restartHint: boolean }[] = [
  { kind: 'microphone', label: 'Microphone', detail: 'Needed to capture your voice.', restartHint: false },
  { kind: 'accessibility', label: 'Accessibility', detail: 'Needed to insert text into other apps.', restartHint: false },
  { kind: 'input_monitoring', label: 'Input Monitoring', detail: 'Needed to detect your global hotkey.', restartHint: true },
];

const stepMap = { permissions: 0, model: 1, ready: 2 };

export function OnboardingFlow() {
  const settings = useAtomValue(settingsAtom);
  const permissions = useAtomValue(permissionsAtom);
  const step = useAtomValue(onboardingStepAtom);
  const recommended = useAtomValue(recommendedModelAtom);
  const restartPrompt = useAtomValue(restartPromptVisibleAtom);
  const restartingApp = useAtomValue(restartingAppAtom);

  const setOnboardingDismissed = useSetAtom(onboardingDismissedAtom);
  const setPermissions = useSetAtom(permissionsAtom);
  const setRestartPrompt = useSetAtom(restartPromptVisibleAtom);
  const setRestartingApp = useSetAtom(restartingAppAtom);
  const armPermissionRecheck = usePermissionRefreshOnFocus();

  const completedCount = permissionJourney.filter((s) => permissions[s.kind] === 'granted').length;
  const nextPerm = permissionJourney.find((s) => permissions[s.kind] !== 'granted') ?? null;

  async function handleRequestPermission(kind: PermissionKind) {
    const isFinal = kind === 'input_monitoring';
    addToast('info', isFinal ? 'Opening macOS Input Monitoring…' : 'Opening macOS Settings…', undefined, 0);
    armPermissionRecheck();
    try {
      await invoke('request_permission', { kind });
      const perms = await invoke<PermissionsStatus>('get_permissions_status');
      setPermissions(perms);
      clearToasts();
      if (isFinal) {
        setRestartPrompt(true);
        addToast('success', 'Final permission opened. Enable it in macOS, then restart ButterVoice.');
      } else {
        addToast('success', 'Permission prompt opened.');
      }
    } catch (error) {
      clearToasts();
      addToast('error', `Failed requesting permission: ${String(error)}`);
    }
  }

  async function handleRefreshPermissions() {
    try {
      const perms = await invoke<PermissionsStatus>('get_permissions_status');
      setPermissions(perms);
      addToast('success', 'Permission states refreshed.');
    } catch (error) {
      addToast('error', `Failed to refresh permissions: ${String(error)}`);
    }
  }

  async function handleDownloadModel() {
    if (settings?.speech_provider === 'remote_openai_compatible') {
      setOnboardingDismissed(true);
      addToast('info', 'Configure your remote speech provider on the Speech settings page.');
      return;
    }
    if (!recommended) return;
    addToast('info', `Downloading ${recommended.id}…`, undefined, 0);
    try {
      await invoke('download_model', { modelId: recommended.id });
      clearToasts();
      addToast('success', `Model '${recommended.id}' downloaded.`);
    } catch (error) {
      clearToasts();
      addToast('error', `Failed downloading model: ${String(error)}`);
    }
  }

  async function handleRestart() {
    setRestartingApp(true);
    addToast('info', 'Restarting ButterVoice…', undefined, 0);
    try {
      await invoke('restart_app');
    } catch (error) {
      setRestartingApp(false);
      clearToasts();
      addToast('error', `Failed restarting app: ${String(error)}`);
    }
  }

  return (
    <Box maw={560} mx="auto" p="xl">
      <Stack gap="lg">
        <Stack align="center" gap={4}>
          <Title order={3}>Welcome to ButterVoice</Title>
          <Text size="sm" c="dimmed">
            Let's get you set up for voice dictation. This takes about a minute.
          </Text>
        </Stack>

        <Stepper active={stepMap[step]} size="sm">
          <Stepper.Step label="Permissions" />
          <Stepper.Step label="Model" />
          <Stepper.Step label="Ready" />
        </Stepper>

        {step === 'permissions' && (
          <Paper p="lg" radius="md" withBorder>
            <Stack gap="md">
              <Text fw={500}>Grant macOS Permissions in Order</Text>
              <Text size="sm" c="dimmed">
                We will guide you through each permission. The final step is Input Monitoring, then you
                can restart once and continue without jumping around.
              </Text>

              <Group justify="space-between">
                <Text size="xs" c="dimmed">{completedCount}/3 complete</Text>
                {nextPerm && <Text size="xs" c="dimmed">Next: {nextPerm.label}</Text>}
              </Group>
              <Progress value={(completedCount / 3) * 100} size="xs" />

              <Stack gap="sm">
                {permissionJourney.map((perm) => {
                  const granted = permissions[perm.kind] === 'granted';
                  const isNext = nextPerm?.kind === perm.kind;
                  const locked = !granted && !isNext;
                  return (
                    <Paper
                      key={perm.kind}
                      p="sm"
                      radius="sm"
                      withBorder
                      style={{
                        opacity: locked ? 0.5 : 1,
                        borderColor: granted
                          ? 'var(--mantine-color-green-4)'
                          : isNext
                            ? 'var(--mantine-color-blue-4)'
                            : undefined,
                        backgroundColor: granted
                          ? 'var(--mantine-color-green-light)'
                          : isNext
                            ? 'var(--mantine-color-blue-light)'
                            : undefined,
                      }}
                    >
                      <Group>
                        <ThemeIcon
                          size="sm"
                          radius="xl"
                          color={granted ? 'green' : 'gray'}
                          variant={granted ? 'filled' : 'light'}
                        >
                          {granted ? <Check size={12} /> : <Text size="xs">{perm.kind === 'microphone' ? '1' : perm.kind === 'accessibility' ? '2' : '3'}</Text>}
                        </ThemeIcon>
                        <Stack gap={0} style={{ flex: 1 }}>
                          <Text size="sm" fw={500}>{perm.label}</Text>
                          <Text size="xs" c="dimmed">{perm.detail}</Text>
                          {perm.restartHint && (
                            <Text size="xs" c="dimmed" fs="italic">
                              Final permission. macOS typically needs an app restart after allowing this.
                            </Text>
                          )}
                        </Stack>
                        <div>
                          {granted ? (
                            <Text size="xs" fw={500} c="green">Granted</Text>
                          ) : isNext ? (
                            <Button size="compact-xs" onClick={() => handleRequestPermission(perm.kind)}>
                              {perm.kind === 'input_monitoring' ? 'Grant Final Permission' : 'Grant'}
                            </Button>
                          ) : (
                            <Text size="xs" c="dimmed">Complete previous step first</Text>
                          )}
                        </div>
                      </Group>
                    </Paper>
                  );
                })}
              </Stack>

              <Button variant="subtle" size="compact-xs" onClick={handleRefreshPermissions} style={{ alignSelf: 'flex-start' }}>
                Recheck Permissions
              </Button>

              {(restartPrompt || (permissions.input_monitoring === 'granted' && !nextPerm)) && (
                <Paper p="md" radius="sm" withBorder style={{ borderColor: 'var(--mantine-color-blue-4)', backgroundColor: 'var(--mantine-color-blue-light)' }}>
                  <Stack gap="xs">
                    <Text size="sm" fw={500}>Final step: restart ButterVoice</Text>
                    <Text size="xs" c="dimmed">
                      Restart once so the hotkey listener is fully initialized with Input Monitoring enabled.
                    </Text>
                    <Button size="xs" onClick={handleRestart} disabled={restartingApp} style={{ alignSelf: 'flex-start' }}>
                      {restartingApp ? 'Restarting…' : 'Restart App'}
                    </Button>
                  </Stack>
                </Paper>
              )}
            </Stack>
          </Paper>
        )}

        {step === 'model' && (
          <Paper p="lg" radius="md" withBorder>
            <Stack gap="md">
              {settings?.speech_provider === 'remote_openai_compatible' ? (
                <>
                  <Text fw={500}>Configure Remote Speech</Text>
                  <Text size="sm" c="dimmed">
                    Remote transcription is selected. Finish setup by adding a base URL and remote model on the Speech settings page.
                  </Text>
                  <Button size="xs" onClick={handleDownloadModel} style={{ alignSelf: 'flex-start' }}>
                    Finish in Speech Settings
                  </Button>
                </>
              ) : (
                <>
                  <Text fw={500}>Download a Speech Model</Text>
                  <Text size="sm" c="dimmed">
                    ButterVoice runs models locally on your Mac. We recommend starting with the default model.
                  </Text>
                  {recommended && (
                    <Paper p="md" radius="sm" withBorder>
                      <Group justify="space-between">
                        <Stack gap={0}>
                          <Text size="sm" fw={500}>{recommended.display_name}</Text>
                          <Text size="xs" c="dimmed">~{recommended.estimated_size_mb} MB</Text>
                        </Stack>
                        <Button size="xs" onClick={handleDownloadModel}>Download</Button>
                      </Group>
                    </Paper>
                  )}
                </>
              )}
            </Stack>
          </Paper>
        )}

        {step === 'ready' && (
          <Paper p="lg" radius="md" withBorder>
            <Stack align="center" gap="md">
              <ThemeIcon size="xl" radius="xl" color="green" variant="light">
                <Check size={24} />
              </ThemeIcon>
              <Text fw={500}>You're all set!</Text>
              <Text size="sm" c="dimmed">
                Hold <Kbd>{settings ? hotkeyDisplayLabel(settings.hotkey) : 'Right Option'}</Kbd> to start dictating.
              </Text>
              <Button onClick={() => setOnboardingDismissed(true)}>Go to Dashboard</Button>
            </Stack>
          </Paper>
        )}

        <Center>
          <Button variant="subtle" size="compact-xs" c="dimmed" onClick={() => setOnboardingDismissed(true)}>
            Skip setup
          </Button>
        </Center>
      </Stack>
    </Box>
  );
}
