import { screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../../../../test/test-utils';
import EventLogPanel from '../EventLogPanel';

vi.mock('../../../../services/coreRpcClient', () => ({
  getCoreHttpBaseUrl: vi.fn().mockResolvedValue('http://localhost:9999'),
  getCoreRpcToken: vi.fn().mockResolvedValue('test-token'),
}));

vi.mock('../../hooks/useSettingsNavigation', () => ({
  useSettingsNavigation: () => ({ navigateBack: vi.fn(), breadcrumbs: [] }),
}));

vi.mock('../../../../lib/i18n/I18nContext', () => ({
  useT: () => ({ t: (k: string) => k }),
}));

function mockFetchSSE(events: Array<{ domain: string; event: string }>) {
  const lines = events.map(e => `data:${JSON.stringify({ ...e, timestamp: '12:00:00' })}\n`);
  const body = lines.join('');
  const encoder = new TextEncoder();
  const stream = new ReadableStream({
    start(controller) {
      controller.enqueue(encoder.encode(body));
      controller.close();
    },
  });
  global.fetch = vi.fn().mockResolvedValue({
    ok: true,
    body: stream,
  });
}

describe('EventLogPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders the panel with header and filter controls', () => {
    global.fetch = vi.fn().mockResolvedValue({ ok: false, body: null });
    renderWithProviders(<EventLogPanel />);
    expect(screen.getByTestId('event-log-panel')).toBeTruthy();
    expect(
      screen.getByText('settings.developerMenu.eventLog.allTypes')
    ).toBeTruthy();
  });

  it('shows waiting message when connected but no events', async () => {
    const stream = new ReadableStream({
      start() {
        // never enqueue — stays open
      },
    });
    global.fetch = vi.fn().mockResolvedValue({ ok: true, body: stream });
    renderWithProviders(<EventLogPanel />);

    await waitFor(() => {
      expect(
        screen.getByText('settings.developerMenu.eventLog.waiting')
      ).toBeTruthy();
    });
  });

  it('renders events from SSE stream with domain badges', async () => {
    mockFetchSSE([
      { domain: 'tool', event: 'ToolExecuted' },
      { domain: 'agent', event: 'AgentStarted' },
    ]);
    renderWithProviders(<EventLogPanel />);

    await waitFor(() => {
      expect(screen.getByText('ToolExecuted')).toBeTruthy();
      expect(screen.getByText('AgentStarted')).toBeTruthy();
    });

    expect(
      screen.getByText('settings.developerMenu.eventLog.badge.tool')
    ).toBeTruthy();
    expect(
      screen.getByText('settings.developerMenu.eventLog.badge.agent')
    ).toBeTruthy();
  });

  it('shows disconnected state when fetch fails', async () => {
    global.fetch = vi.fn().mockRejectedValue(new Error('network'));
    renderWithProviders(<EventLogPanel />);

    await waitFor(() => {
      expect(
        screen.getByText('settings.developerMenu.eventLog.disconnected')
      ).toBeTruthy();
    });
  });

  it('filters events by domain type', async () => {
    mockFetchSSE([
      { domain: 'tool', event: 'ToolA' },
      { domain: 'agent', event: 'AgentB' },
    ]);
    const { container } = renderWithProviders(<EventLogPanel />);

    await waitFor(() => {
      expect(screen.getByText('ToolA')).toBeTruthy();
    });

    const select = container.querySelector('select')!;
    // Simulate selecting 'tool' filter
    const event = new Event('change', { bubbles: true });
    Object.defineProperty(event, 'target', { value: { value: 'tool' } });
    select.dispatchEvent(event);
  });

  it('shows not connected when token is missing', async () => {
    const { getCoreRpcToken } = await import('../../../../services/coreRpcClient');
    vi.mocked(getCoreRpcToken).mockResolvedValueOnce(null as unknown as string);
    global.fetch = vi.fn();
    renderWithProviders(<EventLogPanel />);

    await waitFor(() => {
      expect(
        screen.getByText('settings.developerMenu.eventLog.notConnected')
      ).toBeTruthy();
    });
    expect(global.fetch).not.toHaveBeenCalled();
  });
});
