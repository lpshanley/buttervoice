import { Switch as MantineSwitch } from '@mantine/core';

interface SwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  disabled?: boolean;
}

export function Switch({ checked, onChange, label, disabled }: SwitchProps) {
  return (
    <MantineSwitch
      checked={checked}
      onChange={(event) => onChange(event.currentTarget.checked)}
      aria-label={label}
      disabled={disabled}
    />
  );
}
