import { useState } from 'react';
import { useAtomValue, useSetAtom } from 'jotai';
import { Stack, Text, NativeSelect, Slider, Textarea } from '@mantine/core';
import { settingsAtom } from '../../stores/app';
import { addToast } from '../../stores/toasts';
import { invoke } from '../../lib/tauri';
import type { Settings, SettingsPatch } from '../../types';

const LANGUAGE_OPTIONS = [
  { value: 'en', label: 'English' },
  { value: 'auto', label: 'Auto-detect' },
  { value: 'es', label: 'Spanish' },
  { value: 'fr', label: 'French' },
  { value: 'de', label: 'German' },
  { value: 'it', label: 'Italian' },
  { value: 'pt', label: 'Portuguese' },
  { value: 'ja', label: 'Japanese' },
  { value: 'zh', label: 'Chinese' },
  { value: 'ko', label: 'Korean' },
  { value: 'nl', label: 'Dutch' },
  { value: 'pl', label: 'Polish' },
  { value: 'ru', label: 'Russian' },
  { value: 'sv', label: 'Swedish' },
  { value: 'uk', label: 'Ukrainian' },
];

interface WhisperTuningProps {
  includeLocalControls?: boolean;
}

export function WhisperTuning({ includeLocalControls = true }: WhisperTuningProps) {
  const settings = useAtomValue(settingsAtom);
  const setSettings = useSetAtom(settingsAtom);
  const [localBeamSize, setLocalBeamSize] = useState<number | null>(null);
  const [localNoSpeechThold, setLocalNoSpeechThold] = useState<number | null>(null);

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

  const displayBeamSize = localBeamSize ?? settings.whisper_beam_size;
  const displayNoSpeechThold = localNoSpeechThold ?? settings.whisper_no_speech_thold;

  return (
    <Stack gap="md">
      <NativeSelect
        label="Language"
        description="Force a specific language instead of auto-detection. Forcing a language avoids mis-detection errors."
        size="sm"
        data={LANGUAGE_OPTIONS}
        value={settings.whisper_language}
        onChange={(e) =>
          applyPatch({ whisper_language: e.currentTarget.value }, 'Language updated.')
        }
      />

      {includeLocalControls && (
        <div>
          <Text size="xs" fw={500} c="dimmed" mb="xs">
            Beam Size: {displayBeamSize}
          </Text>
          <Text size="xs" c="dimmed" mb="xs">
            Higher values improve accuracy but increase latency. 1 = greedy decoding.
          </Text>
          <Slider
            min={1}
            max={10}
            step={1}
            value={displayBeamSize}
            onChange={setLocalBeamSize}
            onChangeEnd={(value) => {
              setLocalBeamSize(null);
              applyPatch({ whisper_beam_size: value }, 'Beam size updated.');
            }}
            marks={[
              { value: 1, label: '1' },
              { value: 5, label: '5' },
              { value: 10, label: '10' },
            ]}
          />
        </div>
      )}

      <Textarea
        label="Initial Prompt"
        description="Domain vocabulary, names, or context to guide transcription accuracy."
        size="sm"
        autosize
        minRows={2}
        maxRows={6}
        defaultValue={settings.whisper_prompt}
        placeholder="e.g. ButterVoice, Tauri, macOS, CPAL..."
        onBlur={(e) => {
          if (e.target.value !== settings.whisper_prompt) {
            applyPatch({ whisper_prompt: e.target.value }, 'Initial prompt updated.');
          }
        }}
      />

      {includeLocalControls && (
        <div>
          <Text size="xs" fw={500} c="dimmed" mb="xs">
            No-Speech Threshold: {displayNoSpeechThold.toFixed(2)}
          </Text>
          <Text size="xs" c="dimmed" mb="xs">
            Probability above which a segment is considered non-speech. Raise to reduce hallucinated text in silence.
          </Text>
          <Slider
            min={0}
            max={1}
            step={0.05}
            value={displayNoSpeechThold}
            onChange={setLocalNoSpeechThold}
            onChangeEnd={(value) => {
              setLocalNoSpeechThold(null);
              applyPatch({ whisper_no_speech_thold: value }, 'No-speech threshold updated.');
            }}
            marks={[
              { value: 0, label: '0' },
              { value: 0.5, label: '0.5' },
              { value: 1, label: '1' },
            ]}
          />
        </div>
      )}
    </Stack>
  );
}
