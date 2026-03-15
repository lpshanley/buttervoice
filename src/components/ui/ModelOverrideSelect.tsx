import { useCallback, useMemo, useState } from 'react';
import {
  Combobox,
  InputBase,
  Loader,
  ScrollArea,
  Stack,
  Text,
  Tooltip,
  ActionIcon,
  useCombobox,
} from '@mantine/core';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { RefreshCw } from 'lucide-react';
import { llmModelsQuery } from '../../lib/commands';

interface ModelOverrideSelectProps {
  value: string;
  onChange: (value: string) => void;
  label?: string;
  description?: string;
  placeholder?: string;
}

export function ModelOverrideSelect({
  value,
  onChange,
  label = 'Model Override',
  description = 'Leave empty to use the default model',
  placeholder = 'Optional — search or type a model ID',
}: ModelOverrideSelectProps) {
  const queryClient = useQueryClient();
  const { data: models = [], isLoading: fetching } = useQuery(llmModelsQuery);

  const [search, setSearch] = useState('');
  const [focused, setFocused] = useState(false);

  const combobox = useCombobox({
    onDropdownClose: () => combobox.resetSelectedOption(),
  });

  const fetchModels = useCallback(() => {
    queryClient.invalidateQueries({ queryKey: llmModelsQuery.queryKey });
  }, [queryClient]);

  const filtered = useMemo(() => {
    const q = search.toLowerCase().trim();
    if (!q) return models;
    return models.filter(
      (m) =>
        m.id.toLowerCase().includes(q) ||
        (m.name && m.name.toLowerCase().includes(q)),
    );
  }, [models, search]);

  function select(val: string) {
    onChange(val);
    setSearch('');
    combobox.closeDropdown();
  }

  const searchTrimmed = search.trim();
  const exactMatch = searchTrimmed && models.some((m) => m.id === searchTrimmed);

  const options = filtered.map((m) => (
    <Combobox.Option value={m.id} key={m.id}>
      {m.name && m.name !== m.id ? (
        <Stack gap={0}>
          <Text size="sm" truncate>{m.name}</Text>
          <Text size="xs" c="dimmed" truncate>{m.id}</Text>
        </Stack>
      ) : (
        <Text size="sm" truncate>{m.id}</Text>
      )}
    </Combobox.Option>
  ));

  // Display value: show search when focused, else the current value or empty
  const displayValue = focused ? search : value;

  return (
    <Combobox store={combobox} onOptionSubmit={select}>
      <Combobox.Target>
        <InputBase
          label={label}
          description={description}
          size="sm"
          rightSection={
            fetching ? (
              <Loader size={14} />
            ) : (
              <Tooltip label="Refresh model list">
                <ActionIcon
                  variant="subtle"
                  size="sm"
                  onClick={(e) => {
                    e.stopPropagation();
                    fetchModels();
                  }}
                >
                  <RefreshCw size={14} />
                </ActionIcon>
              </Tooltip>
            )
          }
          value={displayValue}
          onChange={(e) => {
            setSearch(e.currentTarget.value);
            combobox.openDropdown();
            combobox.updateSelectedOptionIndex();
          }}
          onClick={() => combobox.openDropdown()}
          onFocus={() => {
            setFocused(true);
            setSearch('');
            combobox.openDropdown();
          }}
          onBlur={() => {
            setFocused(false);
            combobox.closeDropdown();
            if (searchTrimmed && searchTrimmed !== value) {
              select(searchTrimmed);
            } else {
              setSearch('');
            }
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && searchTrimmed && !exactMatch) {
              e.preventDefault();
              select(searchTrimmed);
            }
          }}
          placeholder={fetching ? 'Loading models…' : placeholder}
        />
      </Combobox.Target>

      <Combobox.Dropdown>
        <Combobox.Options>
          <ScrollArea.Autosize mah={280} type="scroll">
            {/* "Clear override" option when a value is set */}
            {value && (
              <Combobox.Option value="">
                <Text size="sm" fs="italic" c="dimmed">Clear override</Text>
              </Combobox.Option>
            )}
            {fetching ? (
              <Combobox.Empty>Loading models…</Combobox.Empty>
            ) : options.length > 0 ? (
              <>
                {options}
                {searchTrimmed && !exactMatch && (
                  <Combobox.Option value={searchTrimmed}>
                    <Text size="sm" fs="italic">Use &ldquo;{searchTrimmed}&rdquo;</Text>
                  </Combobox.Option>
                )}
              </>
            ) : searchTrimmed ? (
              <Combobox.Option value={searchTrimmed}>
                <Text size="sm" fs="italic">Use &ldquo;{searchTrimmed}&rdquo;</Text>
              </Combobox.Option>
            ) : (
              <Combobox.Empty>No models found</Combobox.Empty>
            )}
          </ScrollArea.Autosize>
        </Combobox.Options>
      </Combobox.Dropdown>
    </Combobox>
  );
}
