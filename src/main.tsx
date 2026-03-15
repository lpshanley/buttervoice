import React from 'react';
import ReactDOM from 'react-dom/client';
import { RouterProvider, createRouter } from '@tanstack/react-router';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createTheme, MantineProvider } from '@mantine/core';
import { Notifications } from '@mantine/notifications';
import { routeTree } from './routeTree.gen';
import './styles/app.css';

const theme = createTheme({
  primaryColor: 'butter',
  colors: {
    butter: [
      '#fefaed',
      '#fef0c7',
      '#fde28a',
      '#fbd04d',
      '#f7be24',
      '#e8a308',
      '#c87d04',
      '#a05a07',
      '#84460e',
      '#713a12',
    ],
  },
  fontFamily: '"Plus Jakarta Sans Variable", "Plus Jakarta Sans", -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, sans-serif',
  headings: {
    fontFamily: '"Plus Jakarta Sans Variable", "Plus Jakarta Sans", -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, sans-serif',
    fontWeight: '600',
  },
  defaultRadius: 'lg',
  cursorType: 'pointer',
  shadows: {
    xs: '0 1px 2px rgb(160 120 60 / 0.06)',
    sm: '0 2px 4px rgb(160 120 60 / 0.06), 0 1px 2px rgb(160 120 60 / 0.04)',
    md: '0 4px 8px -2px rgb(160 120 60 / 0.08), 0 2px 4px -2px rgb(160 120 60 / 0.06)',
    lg: '0 8px 24px -6px rgb(160 120 60 / 0.1), 0 4px 8px -4px rgb(160 120 60 / 0.06)',
    xl: '0 20px 48px -12px rgb(160 120 60 / 0.14), 0 8px 16px -8px rgb(160 120 60 / 0.06)',
  },
});

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5000,
      retry: false,
    },
  },
});

const router = createRouter({ routeTree });

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router;
  }
}

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <MantineProvider theme={theme} defaultColorScheme="auto">
      <Notifications position="bottom-right" />
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    </MantineProvider>
  </React.StrictMode>,
);
