import { useEffect, useRef, useState } from 'react';
import { useAtomValue, useSetAtom } from 'jotai';
import { Stack, Group, Text, NativeSelect, Slider, SimpleGrid, Box } from '@mantine/core';
import { settingsAtom } from '../../stores/app';
import { addToast } from '../../stores/toasts';
import { invoke } from '../../lib/tauri';
import { commands } from '../../lib/commands';
import { usePolling } from '../../lib/hooks/usePolling';
import { MeterBar } from '../ui/MeterBar';
import { tauriAvailable } from '../../lib/tauri';
import { Switch } from '../ui/Switch';
import type { AudioChannelMode, HighPassFilter, Settings, SettingsPatch } from '../../types';

export function SignalProcessing() {
  const settings = useAtomValue(settingsAtom);
  const setSettings = useSetAtom(settingsAtom);
  const [localGain, setLocalGain] = useState<number | null>(null);
  const [inputLevel, setInputLevel] = useState<number>(0);
  const baselineKeepMicStreamOpenRef = useRef<boolean | null>(null);
  const shouldRestoreKeepMicStreamOpenRef = useRef(false);

  useEffect(() => {
    if (!settings || !tauriAvailable) {
      return;
    }

    if (baselineKeepMicStreamOpenRef.current !== null) {
      return;
    }

    baselineKeepMicStreamOpenRef.current = settings.keep_mic_stream_open;
    if (!settings.keep_mic_stream_open) {
      shouldRestoreKeepMicStreamOpenRef.current = true;
      invoke<Settings>('update_settings', {
        patch: { keep_mic_stream_open: true },
      })
        .then((updated) => {
          setSettings(updated);
        })
        .catch(() => {
          shouldRestoreKeepMicStreamOpenRef.current = false;
          addToast('error', 'Unable to enable microphone buffer for gain monitoring.');
        });
    }

    return () => {
      if (!shouldRestoreKeepMicStreamOpenRef.current) {
        return;
      }
      shouldRestoreKeepMicStreamOpenRef.current = false;
      invoke<Settings>('update_settings', {
        patch: { keep_mic_stream_open: baselineKeepMicStreamOpenRef.current ?? false },
      })
        .then(setSettings)
        .catch(() => {});
    };
  }, [settings, setSettings]);

  usePolling(() => {
    if (!tauriAvailable) return;
    commands
      .getAudioInputLevel()
      .then(setInputLevel)
      .catch(() => setInputLevel(0));
  }, 250);

  if (!settings) return null;

  async function applyPatch(patch: SettingsPatch, msg: string) {
    try {
      const updated = await invoke<Settings>('update_settings', { patch });
      setSettings(updated);
      addToast('success', msg);
    } catch (error) {
      addToast('error', `Failed to update settings: ${String(error)}`);
    }
  }

  const displayGain = localGain ?? settings.input_gain_db;

  return (
    <Stack gap="lg">
      {/* ── Input Monitoring ── */}
      <Box>
        <MeterBar level={inputLevel} label="Input Level" />
        <Text size="xs" c="dimmed" mt={6}>
          Adjust gain to keep levels in the green zone during normal speech.
        </Text>
      </Box>

      {/* ── Input Gain ── */}
      <Box>
        <Group justify="space-between" mb={4}>
          <Text size="sm" fw={500}>Input Gain</Text>
          <Text size="xs" ff="monospace" c="dimmed">
            {displayGain > 0 ? '+' : ''}{Math.round(displayGain)} dB
          </Text>
        </Group>
        <Slider
          min={-24}
          max={24}
          step={1}
          value={displayGain}
          onChange={setLocalGain}
          onChangeEnd={(value) => {
            setLocalGain(null);
            applyPatch({ input_gain_db: value }, 'Input gain updated.');
          }}
          marks={[
            { value: -24, label: '-24' },
            { value: 0, label: '0' },
            { value: 24, label: '+24' },
          ]}
        />
      </Box>

      {/* ── Channel & Filter ── */}
      <SimpleGrid cols={2} spacing="md">
        <NativeSelect
          label="Channel Mode"
          size="sm"
          data={[
            { value: 'left', label: 'Left' },
            { value: 'right', label: 'Right' },
            { value: 'mono_mix', label: 'Mono Mix' },
          ]}
          value={settings.audio_channel_mode}
          onChange={(e) => applyPatch({ audio_channel_mode: e.currentTarget.value as AudioChannelMode }, 'Channel mode updated.')}
        />
        <NativeSelect
          label="High Pass Filter"
          size="sm"
          data={[
            { value: 'off', label: 'Off' },
            { value: 'hz80', label: '80 Hz' },
            { value: 'hz120', label: '120 Hz' },
          ]}
          value={settings.high_pass_filter}
          onChange={(e) => applyPatch({ high_pass_filter: e.currentTarget.value as HighPassFilter }, 'Filter updated.')}
        />
      </SimpleGrid>

      {/* ── Microphone Buffer ── */}
      <Group justify="space-between">
        <div>
          <Text size="sm" fw={500}>Microphone Buffer</Text>
          <Text size="xs" c="dimmed">
            Keep a small buffer active to catch early words at dictation start.
          </Text>
        </div>
        <Switch
          checked={settings.keep_mic_stream_open}
          onChange={(v) => applyPatch({ keep_mic_stream_open: v }, 'Microphone buffer updated.')}
          label="Toggle microphone buffer"
        />
      </Group>
    </Stack>
  );
}
