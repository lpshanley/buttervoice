import { useCallback, useEffect, useRef, useState } from 'react';
import { ActionIcon, Tooltip } from '@mantine/core';
import { Play, Pause } from 'lucide-react';
import { commands } from '../../lib/commands';

interface AudioPlayButtonProps {
  recordingFile: string | null;
  size?: 'xs' | 'sm';
}

export function AudioPlayButton({ recordingFile, size = 'xs' }: AudioPlayButtonProps) {
  const [playing, setPlaying] = useState(false);
  const [expired, setExpired] = useState(false);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const blobUrlRef = useRef<string | null>(null);

  const cleanup = useCallback(() => {
    if (audioRef.current) {
      audioRef.current.pause();
      audioRef.current = null;
    }
    if (blobUrlRef.current) {
      URL.revokeObjectURL(blobUrlRef.current);
      blobUrlRef.current = null;
    }
    setPlaying(false);
  }, []);

  useEffect(() => {
    return cleanup;
  }, [cleanup]);

  // Reset when the recording file changes (new dictation)
  useEffect(() => {
    cleanup();
    setExpired(false);
  }, [recordingFile, cleanup]);

  const handleClick = useCallback(async () => {
    if (!recordingFile) return;

    // If already playing, pause
    if (playing && audioRef.current) {
      audioRef.current.pause();
      setPlaying(false);
      return;
    }

    // If we have a cached audio element, resume
    if (audioRef.current && blobUrlRef.current) {
      audioRef.current.play();
      setPlaying(true);
      return;
    }

    // Fetch and play
    try {
      const bytes = await commands.getRecordingAudio(recordingFile);
      const uint8 = new Uint8Array(bytes);
      const blob = new Blob([uint8], { type: 'audio/wav' });
      const url = URL.createObjectURL(blob);
      blobUrlRef.current = url;

      const audio = new Audio(url);
      audioRef.current = audio;

      audio.addEventListener('ended', () => {
        setPlaying(false);
      });

      audio.play();
      setPlaying(true);
    } catch {
      setExpired(true);
      setPlaying(false);
    }
  }, [recordingFile, playing]);

  const disabled = !recordingFile || expired;
  const tooltip = expired
    ? 'Recording expired'
    : !recordingFile
      ? 'No recording'
      : playing
        ? 'Pause playback'
        : 'Play recording';

  return (
    <Tooltip label={tooltip}>
      <ActionIcon
        variant="subtle"
        size={size}
        disabled={disabled}
        onClick={handleClick}
        aria-label={tooltip}
      >
        {playing ? <Pause size={14} /> : <Play size={14} />}
      </ActionIcon>
    </Tooltip>
  );
}
