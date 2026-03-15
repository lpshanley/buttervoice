import { useEffect, useMemo } from 'react';
import { useAtomValue, useSetAtom } from 'jotai';
import { Check, X, ChevronUp, ChevronDown, ChevronsUpDown, Mic, Cpu, SlidersHorizontal, Server } from 'lucide-react';
import {
  Stack, Group, Text, Paper, Badge, Radio, Button, Center,
  Progress, ActionIcon, Box, SimpleGrid, UnstyledButton,
} from '@mantine/core';
import {
  useReactTable,
  getCoreRowModel,
  getSortedRowModel,
  flexRender,
  createColumnHelper,
  type SortingState,
} from '@tanstack/react-table';
import { SectionCard } from '../ui/SectionCard';
import {
  settingsAtom,
  modelsAtom,
  downloadedModelsAtom,
  downloadingModelsAtom,
  deletingModelsAtom,
  downloadProgressAtom,
} from '../../stores/app';
import { addToast, clearToasts } from '../../stores/toasts';
import { invoke, listen } from '../../lib/tauri';
import { WhisperTuning } from '../settings/WhisperTuning';
import { RuntimeSettings } from '../settings/RuntimeSettings';
import { SpeechRemoteSettings } from '../settings/SpeechRemoteSettings';
import type { ModelDownloadProgress, ModelInfo, Settings, SpeechProvider } from '../../types';

function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

const columnHelper = createColumnHelper<ModelInfo>();

function SortIcon({ sorted }: { sorted: false | 'asc' | 'desc' }) {
  if (sorted === 'asc') return <ChevronUp size={14} />;
  if (sorted === 'desc') return <ChevronDown size={14} />;
  return <ChevronsUpDown size={14} style={{ opacity: 0.3 }} />;
}

export function VoiceModels() {
  const settings = useAtomValue(settingsAtom);
  const models = useAtomValue(modelsAtom);
  const downloadedModels = useAtomValue(downloadedModelsAtom);
  const downloadingModels = useAtomValue(downloadingModelsAtom);
  const deletingModels = useAtomValue(deletingModelsAtom);
  const downloadProgress = useAtomValue(downloadProgressAtom);

  const setSettings = useSetAtom(settingsAtom);
  const setDownloadedModels = useSetAtom(downloadedModelsAtom);
  const setDownloadingModels = useSetAtom(downloadingModelsAtom);
  const setDeletingModels = useSetAtom(deletingModelsAtom);
  const setDownloadProgress = useSetAtom(downloadProgressAtom);
  const isLocalProvider = settings?.speech_provider === 'local_whispercpp';

  useEffect(() => {
    const unlisten = listen<ModelDownloadProgress>('model-download-progress', (payload) => {
      const { model_id, status } = payload;

      if (status === 'downloading') {
        setDownloadProgress((prev) => {
          const next = new Map(prev);
          next.set(model_id, payload);
          return next;
        });
      } else if (status === 'completed') {
        setDownloadProgress((prev) => {
          const next = new Map(prev);
          next.delete(model_id);
          return next;
        });
        setDownloadingModels((prev) => { const next = new Set(prev); next.delete(model_id); return next; });
        setDownloadedModels((prev) => new Set([...prev, model_id]));
        clearToasts();
        addToast('success', `Model '${model_id}' downloaded.`);
      } else if (status === 'cancelled') {
        setDownloadProgress((prev) => {
          const next = new Map(prev);
          next.delete(model_id);
          return next;
        });
        setDownloadingModels((prev) => { const next = new Set(prev); next.delete(model_id); return next; });
        clearToasts();
        addToast('info', `Download of '${model_id}' cancelled.`);
      } else if (status === 'error') {
        setDownloadProgress((prev) => {
          const next = new Map(prev);
          next.delete(model_id);
          return next;
        });
        setDownloadingModels((prev) => { const next = new Set(prev); next.delete(model_id); return next; });
        clearToasts();
        addToast('error', `Failed downloading model: ${payload.error ?? 'unknown error'}`);
      }
    });

    return () => { unlisten.then((fn) => fn()); };
  }, [setDownloadProgress, setDownloadingModels, setDownloadedModels]);

  async function downloadModel(modelId: string) {
    if (downloadingModels.has(modelId)) return;
    setDownloadingModels((prev: Set<string>) => new Set([...prev, modelId]));
    addToast('info', `Downloading ${modelId}…`, undefined, 0);
    try {
      await invoke('start_model_download', { modelId });
    } catch (error) {
      clearToasts();
      addToast('error', `Failed starting download: ${String(error)}`);
      setDownloadingModels((prev: Set<string>) => { const next = new Set(prev); next.delete(modelId); return next; });
    }
  }

  async function cancelDownload(modelId: string) {
    try {
      await invoke('cancel_model_download', { modelId });
    } catch {
      // Cancel is best-effort; the event listener handles cleanup
    }
  }

  async function activateModel(modelId: string) {
    if (!downloadedModels.has(modelId) || settings?.model_id === modelId) return;
    try {
      const updated = await invoke<Settings>('update_settings', { patch: { model_id: modelId } });
      setSettings(updated);
      addToast('success', `Selected model '${modelId}'.`);
    } catch (error) {
      addToast('error', `Failed to update settings: ${String(error)}`);
    }
  }

  async function switchProvider(provider: SpeechProvider) {
    if (!settings || settings.speech_provider === provider) return;
    try {
      const updated = await invoke<Settings>('update_settings', {
        patch: { speech_provider: provider },
      });
      setSettings(updated);
      addToast(
        'success',
        provider === 'local_whispercpp'
          ? 'Local speech provider selected.'
          : 'Remote speech provider selected.',
      );
    } catch (error) {
      addToast('error', `Failed to update settings: ${String(error)}`);
    }
  }

  async function deleteModel(modelId: string) {
    if (settings?.model_id === modelId) {
      addToast('error', 'Cannot delete the active model. Activate another downloaded model first.');
      return;
    }
    if (deletingModels.has(modelId)) return;
    setDeletingModels((prev: Set<string>) => new Set([...prev, modelId]));
    try {
      await invoke('delete_model', { modelId });
      setDownloadedModels((prev: Set<string>) => { const next = new Set(prev); next.delete(modelId); return next; });
      addToast('success', `Model '${modelId}' deleted.`);
    } catch (error) {
      addToast('error', `Failed deleting model: ${String(error)}`);
    } finally {
      setDeletingModels((prev: Set<string>) => { const next = new Set(prev); next.delete(modelId); return next; });
    }
  }

  const columns = useMemo(() => [
    columnHelper.accessor('display_name', {
      id: 'model',
      header: 'Model',
      enableSorting: false,
      cell: ({ row }) => {
        const model = row.original;
        const isDownloaded = downloadedModels.has(model.id);
        const isActive = settings?.model_id === model.id;
        return (
          <Group gap="sm">
            <Radio
              name="active-model"
              checked={isActive}
              disabled={!isDownloaded}
              onChange={() => activateModel(model.id)}
              aria-label={`Set ${model.display_name} as active model`}
            />
            <Stack gap={0}>
              <Group gap="xs">
                <Text size="sm" fw={500}>{model.display_name}</Text>
                {model.recommended && <Badge size="xs" variant="light">Recommended</Badge>}
                {isActive && <Badge size="xs" color="green" variant="light">Active</Badge>}
              </Group>
              <Text size="xs" c="dimmed">{model.id}</Text>
            </Stack>
          </Group>
        );
      },
    }),
    columnHelper.accessor('estimated_size_mb', {
      id: 'size',
      header: 'Size',
      sortingFn: (rowA, rowB) => {
        const a = rowA.original;
        const b = rowB.original;
        if (a.family_order !== b.family_order) return a.family_order - b.family_order;
        return a.estimated_size_mb - b.estimated_size_mb;
      },
      cell: ({ getValue }) => {
        const mb = getValue();
        return (
          <Text size="xs" c="dimmed" ta="center">
            {mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${mb} MB`}
          </Text>
        );
      },
    }),
    columnHelper.accessor('quantized', {
      id: 'quantized',
      header: 'Quantized',
      sortingFn: (rowA, rowB) => {
        const a = rowA.original;
        const b = rowB.original;
        if (a.family_order !== b.family_order) return a.family_order - b.family_order;
        const aVal = a.quantized ? 0 : 1;
        const bVal = b.quantized ? 0 : 1;
        if (aVal !== bVal) return aVal - bVal;
        return a.estimated_size_mb - b.estimated_size_mb;
      },
      cell: ({ getValue }) =>
        getValue() ? (
          <Center><Check size={12} color="var(--mantine-color-green-6)" /></Center>
        ) : null,
    }),
    columnHelper.display({
      id: 'actions',
      header: '',
      cell: ({ row }) => {
        const model = row.original;
        const isDownloaded = downloadedModels.has(model.id);
        const isActive = settings?.model_id === model.id;
        const isDownloading = downloadingModels.has(model.id);
        const isDeleting = deletingModels.has(model.id);
        const progress = downloadProgress.get(model.id);

        if (isDownloaded) {
          if (isActive) return null;
          return (
            <Group justify="center">
              <Button
                variant="subtle"
                color="red"
                size="compact-xs"
                disabled={isDeleting}
                onClick={() => deleteModel(model.id)}
              >
                {isDeleting ? 'Deleting…' : 'Delete'}
              </Button>
            </Group>
          );
        }

        if (isDownloading && progress) {
          return (
            <Group gap="xs" wrap="nowrap" style={{ minWidth: 160 }}>
              <Stack gap={2} style={{ flex: 1 }}>
                <Progress
                  value={progress.total_bytes > 0
                    ? (progress.downloaded_bytes / progress.total_bytes) * 100
                    : 0}
                  size="sm"
                  animated
                />
                <Text size="xs" c="dimmed">
                  {progress.total_bytes > 0
                    ? `${formatBytes(progress.downloaded_bytes)} / ${formatBytes(progress.total_bytes)}`
                    : formatBytes(progress.downloaded_bytes)}
                </Text>
              </Stack>
              <ActionIcon
                variant="subtle"
                color="red"
                size="sm"
                onClick={() => cancelDownload(model.id)}
                aria-label={`Cancel download of ${model.display_name}`}
              >
                <X size={14} />
              </ActionIcon>
            </Group>
          );
        }

        return (
          <Group justify="center">
            <Button
              variant="subtle"
              size="compact-xs"
              disabled={isDownloading}
              onClick={() => downloadModel(model.id)}
            >
              {isDownloading ? 'Starting…' : 'Download'}
            </Button>
          </Group>
        );
      },
    }),
  // eslint-disable-next-line react-hooks/exhaustive-deps
  ], [downloadedModels, downloadingModels, deletingModels, downloadProgress, settings]);

  const defaultSorting: SortingState = [{ id: 'size', desc: false }];

  const table = useReactTable({
    data: models,
    columns,
    initialState: { sorting: defaultSorting },
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getRowId: (row) => row.id,
  });

  return (
    <Box p="xl" maw={720} mx="auto">
      <Stack gap="lg">
      <SectionCard icon={Mic} title="Speech Provider">
        <Stack gap="md">
          <Text size="xs" c="dimmed">
            Choose whether transcription runs locally with whisper.cpp or through a remote OpenAI-compatible speech API such as Speaches.
          </Text>

          <SimpleGrid cols={2}>
            <UnstyledButton onClick={() => switchProvider('local_whispercpp')}>
              <Paper
                p="md"
                radius="md"
                withBorder
                style={{
                  borderColor: isLocalProvider ? 'var(--mantine-color-blue-6)' : undefined,
                  backgroundColor: isLocalProvider ? 'var(--mantine-color-blue-light)' : undefined,
                }}
              >
                <Stack gap={4} align="center" ta="center">
                  <Mic size={18} color="var(--mantine-color-dimmed)" />
                  <Text size="sm" fw={500}>Local</Text>
                  <Text size="xs" c="dimmed">Run whisper.cpp on this Mac.</Text>
                </Stack>
              </Paper>
            </UnstyledButton>
            <UnstyledButton onClick={() => switchProvider('remote_openai_compatible')}>
              <Paper
                p="md"
                radius="md"
                withBorder
                style={{
                  borderColor: !isLocalProvider ? 'var(--mantine-color-blue-6)' : undefined,
                  backgroundColor: !isLocalProvider ? 'var(--mantine-color-blue-light)' : undefined,
                }}
              >
                <Stack gap={4} align="center" ta="center">
                  <Server size={18} color="var(--mantine-color-dimmed)" />
                  <Text size="sm" fw={500}>Remote</Text>
                  <Text size="xs" c="dimmed">Send audio to Speaches, OpenAI, or a custom hosted API.</Text>
                </Stack>
              </Paper>
            </UnstyledButton>
          </SimpleGrid>
        </Stack>
      </SectionCard>

      {isLocalProvider ? (
      <SectionCard icon={Mic} title="Local Models">
        <Stack gap="md">
          <Group justify="space-between">
            <Text size="xs" c="dimmed">
              Download models you do not have yet, then activate any downloaded model from this list.
            </Text>
            <Group gap="sm">
              <Text size="xs" c="dimmed">{downloadedModels.size} downloaded</Text>
              <Text size="xs" c="dimmed">{models.length} available</Text>
            </Group>
          </Group>

          <Paper radius="md" withBorder style={{ overflow: 'hidden' }}>
            <Box component="table" style={{ width: '100%', borderCollapse: 'collapse' }}>
              <thead>
                {table.getHeaderGroups().map((headerGroup) => (
                  <tr key={headerGroup.id}>
                    {headerGroup.headers.map((header) => {
                      const canSort = header.column.getCanSort();
                      return (
                        <th
                          key={header.id}
                          style={{
                            padding: 'var(--mantine-spacing-sm) var(--mantine-spacing-md)',
                            textAlign: header.id === 'model' ? 'left' : 'center',
                            cursor: canSort ? 'pointer' : 'default',
                            userSelect: canSort ? 'none' : undefined,
                            fontWeight: 500,
                            fontSize: 'var(--mantine-font-size-sm)',
                            color: 'var(--mantine-color-dimmed)',
                            borderBottom: '1px solid var(--mantine-color-default-border)',
                          }}
                          onClick={canSort ? header.column.getToggleSortingHandler() : undefined}
                        >
                          <Group
                            gap={4}
                            justify={header.id === 'model' ? 'flex-start' : 'center'}
                            wrap="nowrap"
                          >
                            {flexRender(header.column.columnDef.header, header.getContext())}
                            {canSort && <SortIcon sorted={header.column.getIsSorted()} />}
                          </Group>
                        </th>
                      );
                    })}
                  </tr>
                ))}
              </thead>
              <tbody>
                {table.getRowModel().rows.map((row) => {
                  const isActive = settings?.model_id === row.original.id;
                  return (
                    <tr
                      key={row.id}
                      style={{
                        backgroundColor: isActive ? 'var(--mantine-color-blue-light)' : undefined,
                      }}
                    >
                      {row.getVisibleCells().map((cell) => (
                        <td
                          key={cell.id}
                          style={{
                            padding: 'var(--mantine-spacing-sm) var(--mantine-spacing-md)',
                            textAlign: cell.column.id === 'model' ? 'left' : 'center',
                          }}
                        >
                          {flexRender(cell.column.columnDef.cell, cell.getContext())}
                        </td>
                      ))}
                    </tr>
                  );
                })}
              </tbody>
            </Box>
            {models.length === 0 && (
              <Center py="xl">
                <Text size="sm" c="dimmed">No models available.</Text>
              </Center>
            )}
          </Paper>
        </Stack>
      </SectionCard>
      ) : (
      <SectionCard icon={Server} title="Remote Provider">
        <SpeechRemoteSettings />
      </SectionCard>
      )}

      <SectionCard icon={SlidersHorizontal} title={isLocalProvider ? 'Speech Tuning' : 'Shared Speech Tuning'}>
        <WhisperTuning includeLocalControls={Boolean(isLocalProvider)} />
      </SectionCard>

      {isLocalProvider && (
        <SectionCard icon={Cpu} title="Compute">
          <RuntimeSettings />
        </SectionCard>
      )}
      </Stack>
    </Box>
  );
}
