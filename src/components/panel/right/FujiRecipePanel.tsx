import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { toast } from 'react-toastify';
import { Camera, RefreshCw, Trash2, Play } from 'lucide-react';
import { useEditorStore } from '../../store/useEditorStore';
import { Invokes } from '../ui/AppProperties';
import Text from '../ui/Text';
import { TextVariants } from '../ui/Types';

export type FujiRecipe = {
  filmSimulation: number;
  dynamicRange: number;
  highlightTone: number;
  shadowTone: number;
  color: number;
  sharpness: number;
  grain: 'off' | 'weakSmall' | 'strongSmall' | 'weakLarge' | 'strongLarge';
  clarity: number;
  colorChrome: 'off' | 'weak' | 'strong';
  colorChromeFxBlue: 'off' | 'weak' | 'strong';
  whiteBalance: number;
  wbShiftR: number;
  wbShiftB: number;
  colorTempK: number;
  highIsoNr: number;
  monoWc: number;
  monoMg: number;
  smoothSkin: 'off' | 'weak' | 'strong';
  longExpNr: boolean;
  colorSpaceSrgb: boolean;
  imageSize: number;
  imageQuality: number;
};

type SupportInfo = {
  supported: boolean;
  featureEnabled: boolean;
  platform: { os: string; title: string; steps: string[]; requiresWinusb: boolean };
};

type DiscoveredCamera = {
  busId: string;
  modelName: string;
  verified: boolean;
  untestedWarning?: string | null;
};

type QueueJob = {
  id: string;
  sourcePath: string;
  cacheKey: string;
  status: string;
  error?: string | null;
};

const FILM_SIMS: Array<{ value: number; label: string; mono?: boolean }> = [
  { value: 0x01, label: 'Provia' },
  { value: 0x02, label: 'Velvia' },
  { value: 0x03, label: 'Astia' },
  { value: 0x04, label: 'PRO Neg. Hi' },
  { value: 0x05, label: 'PRO Neg. Std' },
  { value: 0x0b, label: 'Classic Chrome' },
  { value: 0x11, label: 'Classic Neg.' },
  { value: 0x13, label: 'Nostalgic Neg.' },
  { value: 0x14, label: 'Reala Ace' },
  { value: 0x10, label: 'Eterna' },
  { value: 0x12, label: 'Eterna Bleach Bypass' },
  { value: 0x0c, label: 'Acros', mono: true },
  { value: 0x0d, label: 'Acros+Ye', mono: true },
  { value: 0x0e, label: 'Acros+R', mono: true },
  { value: 0x0f, label: 'Acros+G', mono: true },
  { value: 0x06, label: 'Monochrome', mono: true },
  { value: 0x0a, label: 'Sepia', mono: true },
];

const DEFAULT_RECIPE: FujiRecipe = {
  filmSimulation: 0x01,
  dynamicRange: 100,
  highlightTone: 0,
  shadowTone: 0,
  color: 0,
  sharpness: 0,
  grain: 'off',
  clarity: 0,
  colorChrome: 'off',
  colorChromeFxBlue: 'off',
  whiteBalance: 0,
  wbShiftR: 0,
  wbShiftB: 0,
  colorTempK: 6500,
  highIsoNr: 0,
  monoWc: 0,
  monoMg: 0,
  smoothSkin: 'off',
  longExpNr: true,
  colorSpaceSrgb: true,
  imageSize: 0x07,
  imageQuality: 0x02,
};

function statusLabel(status: string | undefined, t: (k: string) => string): string {
  switch (status) {
    case 'ready':
      return t('editor.fujiRecipe.status.ready');
    case 'queued':
      return t('editor.fujiRecipe.status.queued');
    case 'stale':
      return t('editor.fujiRecipe.status.stale');
    case 'failed':
      return t('editor.fujiRecipe.status.failed');
    default:
      return t('editor.fujiRecipe.status.none');
  }
}

export default function FujiRecipePanel() {
  const { t } = useTranslation();
  const selectedImage = useEditorStore((s) => s.selectedImage);
  const adjustments = useEditorStore((s) => s.adjustments);
  const setAdjustments = useEditorStore((s) => s.setAdjustments);

  const [support, setSupport] = useState<SupportInfo | null>(null);
  const [cameras, setCameras] = useState<DiscoveredCamera[]>([]);
  const [connected, setConnected] = useState<DiscoveredCamera | null>(null);
  const [queue, setQueue] = useState<QueueJob[]>([]);
  const [busy, setBusy] = useState(false);

  const isCameraRender = Boolean(adjustments?.fujiCameraRender);
  const renderStatus = (adjustments?.fujiRenderStatus as string) || 'none';

  const recipe: FujiRecipe = useMemo(() => {
    return { ...DEFAULT_RECIPE, ...(adjustments?.fujiRecipe || {}) };
  }, [adjustments?.fujiRecipe]);

  const isMono = useMemo(
    () => FILM_SIMS.find((s) => s.value === recipe.filmSimulation)?.mono === true,
    [recipe.filmSimulation],
  );

  const updateRecipe = useCallback(
    (patch: Partial<FujiRecipe>) => {
      setAdjustments((prev: any) => {
        const nextRecipe = { ...DEFAULT_RECIPE, ...(prev.fujiRecipe || {}), ...patch };
        let status = prev.fujiRenderStatus || 'none';
        if (status === 'ready') status = 'stale';
        return {
          ...prev,
          fujiRecipe: nextRecipe,
          fujiRenderStatus: status === 'none' ? 'none' : status,
        };
      });
    },
    [setAdjustments],
  );

  const refreshSupport = useCallback(async () => {
    try {
      const info = await invoke<SupportInfo>(Invokes.IsFujiRawConvSupported);
      setSupport(info);
    } catch (e) {
      console.error(e);
    }
  }, []);

  const refreshQueue = useCallback(async () => {
    try {
      const jobs = await invoke<QueueJob[]>(Invokes.FujiListQueue);
      setQueue(jobs);
    } catch {
      /* ignore when unsupported */
    }
  }, []);

  useEffect(() => {
    refreshSupport();
    refreshQueue();
  }, [refreshSupport, refreshQueue]);

  useEffect(() => {
    const path = selectedImage?.path;
    if (!path || adjustments?.fujiRecipe) return;
    if (!path.toLowerCase().includes('.raf')) return;
    const sourcePath = path.split('?')[0];
    invoke<FujiRecipe>(Invokes.FujiParseRecipeFromRaf, { path: sourcePath })
      .then((parsed) => {
        setAdjustments((prev: any) => ({
          ...prev,
          fujiRecipe: { ...DEFAULT_RECIPE, ...parsed },
          fujiRenderStatus: prev.fujiRenderStatus || 'none',
        }));
      })
      .catch(() => {
        setAdjustments((prev: any) => ({
          ...prev,
          fujiRecipe: DEFAULT_RECIPE,
          fujiRenderStatus: prev.fujiRenderStatus || 'none',
        }));
      });
  }, [selectedImage?.path, adjustments?.fujiRecipe, setAdjustments]);

  const handleListCameras = async () => {
    setBusy(true);
    try {
      const list = await invoke<DiscoveredCamera[]>(Invokes.FujiListCameras);
      setCameras(list);
      if (list.length === 0 && support?.platform) {
        toast.info(support.platform.title);
      }
    } catch (e: any) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleConnect = async (busId: string) => {
    setBusy(true);
    try {
      const cam = await invoke<DiscoveredCamera>(Invokes.FujiConnect, { busId });
      setConnected(cam);
      if (cam.untestedWarning) toast.warn(cam.untestedWarning);
      toast.success(t('editor.fujiRecipe.connected', { model: cam.modelName }));
    } catch (e: any) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleRecover = async () => {
    setBusy(true);
    try {
      await invoke(Invokes.FujiRecoverSession);
      toast.success(t('editor.fujiRecipe.recovered'));
    } catch (e: any) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleEnqueue = async () => {
    if (!selectedImage?.path) return;
    setBusy(true);
    try {
      const sourcePath = selectedImage.path.split('?')[0];
      const job = await invoke<QueueJob>(Invokes.FujiEnqueueConvert, {
        sourcePath,
        recipe,
      });
      setAdjustments((prev: any) => ({
        ...prev,
        fujiCacheKey: job.cacheKey,
        fujiRenderStatus: job.status,
      }));
      await refreshQueue();
      if (job.status === 'ready') {
        toast.success(t('editor.fujiRecipe.cacheHit'));
      } else {
        toast.info(t('editor.fujiRecipe.queued'));
      }
    } catch (e: any) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleProcessQueue = async () => {
    setBusy(true);
    try {
      const results = await invoke<Array<{ cacheKey: string; status: string; error?: string }>>(
        Invokes.FujiProcessQueue,
      );
      await refreshQueue();
      const ready = results.find((r) => r.status === 'ready');
      if (ready) {
        setAdjustments((prev: any) => ({
          ...prev,
          fujiCacheKey: ready.cacheKey,
          fujiRenderStatus: 'ready',
        }));
        toast.success(t('editor.fujiRecipe.renderReady'));
      } else if (results[0]?.error) {
        toast.error(results[0].error);
      }
    } catch (e: any) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleCreateVersion = async () => {
    if (!selectedImage?.path || !adjustments?.fujiCacheKey) return;
    setBusy(true);
    try {
      const path = await invoke<string>(Invokes.FujiCreateCameraRenderVersion, {
        sourceVirtualPath: selectedImage.path,
        recipe,
        cacheKey: adjustments.fujiCacheKey,
      });
      toast.success(t('editor.fujiRecipe.versionCreated', { path }));
    } catch (e: any) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handlePurgeCache = async () => {
    try {
      const bytes = await invoke<number>(Invokes.FujiPurgeCache);
      toast.info(t('editor.fujiRecipe.cachePurged', { bytes }));
    } catch (e: any) {
      toast.error(String(e));
    }
  };

  if (support && !support.featureEnabled) {
    return (
      <div className="p-3 space-y-3 text-sm text-text-secondary">
        <Text variant={TextVariants.heading}>{t('editor.fujiRecipe.title')}</Text>
        <p>{t('editor.fujiRecipe.notInBuild')}</p>
        <p className="text-xs">{support.platform.title}</p>
      </div>
    );
  }

  return (
    <div className="p-3 space-y-4 text-sm">
      <div className="flex items-center justify-between gap-2">
        <Text variant={TextVariants.heading}>{t('editor.fujiRecipe.title')}</Text>
        <span className="text-xs text-text-secondary">{statusLabel(renderStatus, t)}</span>
      </div>

      {isCameraRender && (
        <p className="text-xs text-text-secondary border border-surface rounded-md p-2">
          {t('editor.fujiRecipe.cameraRenderHint')}
        </p>
      )}

      <section className="space-y-2">
        <Text variant={TextVariants.heading}>{t('editor.fujiRecipe.camera')}</Text>
        <div className="flex flex-wrap gap-2">
          <button
            className="px-2 py-1 rounded-md bg-bg-tertiary hover:bg-surface disabled:opacity-50"
            disabled={busy}
            onClick={handleListCameras}
          >
            <Camera className="inline w-4 h-4 mr-1" />
            {t('editor.fujiRecipe.scan')}
          </button>
          <button
            className="px-2 py-1 rounded-md bg-bg-tertiary hover:bg-surface disabled:opacity-50"
            disabled={busy || !connected}
            onClick={handleRecover}
          >
            <RefreshCw className="inline w-4 h-4 mr-1" />
            {t('editor.fujiRecipe.recover')}
          </button>
        </div>
        {cameras.map((cam) => (
          <button
            key={cam.busId}
            className="block w-full text-left px-2 py-1 rounded-md bg-bg-tertiary hover:bg-surface"
            onClick={() => handleConnect(cam.busId)}
          >
            {cam.modelName}
            {!cam.verified && (
              <span className="ml-2 text-xs text-text-secondary">
                ({t('editor.fujiRecipe.untested')})
              </span>
            )}
          </button>
        ))}
        {connected && (
          <p className="text-xs text-text-secondary">
            {t('editor.fujiRecipe.connected', { model: connected.modelName })}
          </p>
        )}
        {support?.platform?.requiresWinusb && (
          <div className="text-xs text-text-secondary space-y-1">
            <p className="font-medium">{support.platform.title}</p>
            <ol className="list-decimal pl-4 space-y-0.5">
              {support.platform.steps.map((step) => (
                <li key={step}>{step}</li>
              ))}
            </ol>
          </div>
        )}
      </section>

      <section className="space-y-2">
        <Text variant={TextVariants.heading}>{t('editor.fujiRecipe.recipe')}</Text>
        <label className="block">
          <span className="text-xs text-text-secondary">{t('editor.fujiRecipe.filmSim')}</span>
          <select
            className="w-full mt-1 bg-bg-tertiary rounded-md px-2 py-1"
            value={recipe.filmSimulation}
            disabled={isCameraRender}
            onChange={(e) => updateRecipe({ filmSimulation: Number(e.target.value) })}
          >
            {FILM_SIMS.map((s) => (
              <option key={s.value} value={s.value}>
                {s.label}
              </option>
            ))}
          </select>
        </label>

        <label className="block">
          <span className="text-xs text-text-secondary">{t('editor.fujiRecipe.dynamicRange')}</span>
          <select
            className="w-full mt-1 bg-bg-tertiary rounded-md px-2 py-1"
            value={recipe.dynamicRange}
            disabled={isCameraRender}
            onChange={(e) => updateRecipe({ dynamicRange: Number(e.target.value) })}
          >
            <option value={100}>DR100</option>
            <option value={200}>DR200</option>
            <option value={400}>DR400</option>
          </select>
        </label>

        {(
          [
            ['highlightTone', t('editor.fujiRecipe.highlightTone')],
            ['shadowTone', t('editor.fujiRecipe.shadowTone')],
            ['sharpness', t('editor.fujiRecipe.sharpness')],
            ['clarity', t('editor.fujiRecipe.clarity')],
          ] as Array<[keyof FujiRecipe, string]>
        ).map(([key, label]) => (
          <label key={key} className="block">
            <span className="text-xs text-text-secondary">
              {label}: {Number(recipe[key]).toFixed(1)}
            </span>
            <input
              type="range"
              min={-4}
              max={4}
              step={0.5}
              className="w-full"
              disabled={isCameraRender}
              value={Number(recipe[key])}
              onChange={(e) => updateRecipe({ [key]: Number(e.target.value) } as any)}
            />
          </label>
        ))}

        {!isMono && (
          <label className="block">
            <span className="text-xs text-text-secondary">
              {t('editor.fujiRecipe.color')}: {recipe.color.toFixed(1)}
            </span>
            <input
              type="range"
              min={-4}
              max={4}
              step={0.5}
              className="w-full"
              disabled={isCameraRender}
              value={recipe.color}
              onChange={(e) => updateRecipe({ color: Number(e.target.value) })}
            />
          </label>
        )}

        {isMono && (
          <>
            <label className="block">
              <span className="text-xs text-text-secondary">
                {t('editor.fujiRecipe.monoWc')}: {recipe.monoWc.toFixed(1)}
              </span>
              <input
                type="range"
                min={-9}
                max={9}
                step={1}
                className="w-full"
                disabled={isCameraRender}
                value={recipe.monoWc}
                onChange={(e) => updateRecipe({ monoWc: Number(e.target.value) })}
              />
            </label>
            <label className="block">
              <span className="text-xs text-text-secondary">
                {t('editor.fujiRecipe.monoMg')}: {recipe.monoMg.toFixed(1)}
              </span>
              <input
                type="range"
                min={-9}
                max={9}
                step={1}
                className="w-full"
                disabled={isCameraRender}
                value={recipe.monoMg}
                onChange={(e) => updateRecipe({ monoMg: Number(e.target.value) })}
              />
            </label>
          </>
        )}

        <label className="block">
          <span className="text-xs text-text-secondary">{t('editor.fujiRecipe.grain')}</span>
          <select
            className="w-full mt-1 bg-bg-tertiary rounded-md px-2 py-1"
            value={recipe.grain}
            disabled={isCameraRender}
            onChange={(e) => updateRecipe({ grain: e.target.value as FujiRecipe['grain'] })}
          >
            <option value="off">Off</option>
            <option value="weakSmall">Weak / Small</option>
            <option value="strongSmall">Strong / Small</option>
            <option value="weakLarge">Weak / Large</option>
            <option value="strongLarge">Strong / Large</option>
          </select>
        </label>

        <label className="block">
          <span className="text-xs text-text-secondary">{t('editor.fujiRecipe.colorChrome')}</span>
          <select
            className="w-full mt-1 bg-bg-tertiary rounded-md px-2 py-1"
            value={recipe.colorChrome}
            disabled={isCameraRender}
            onChange={(e) =>
              updateRecipe({ colorChrome: e.target.value as FujiRecipe['colorChrome'] })
            }
          >
            <option value="off">Off</option>
            <option value="weak">Weak</option>
            <option value="strong">Strong</option>
          </select>
        </label>

        <label className="block">
          <span className="text-xs text-text-secondary">{t('editor.fujiRecipe.wb')}</span>
          <select
            className="w-full mt-1 bg-bg-tertiary rounded-md px-2 py-1"
            value={recipe.whiteBalance}
            disabled={isCameraRender}
            onChange={(e) => updateRecipe({ whiteBalance: Number(e.target.value) })}
          >
            <option value={0x0000}>As Shot</option>
            <option value={0x0002}>Auto</option>
            <option value={0x0004}>Daylight</option>
            <option value={0x8006}>Shade</option>
            <option value={0x0006}>Incandescent</option>
            <option value={0x8007}>Color Temperature</option>
          </select>
        </label>

        {recipe.whiteBalance === 0x8007 && (
          <label className="block">
            <span className="text-xs text-text-secondary">
              {t('editor.fujiRecipe.colorTemp')}: {recipe.colorTempK} K
            </span>
            <input
              type="range"
              min={2500}
              max={10000}
              step={10}
              className="w-full"
              disabled={isCameraRender}
              value={recipe.colorTempK}
              onChange={(e) => updateRecipe({ colorTempK: Number(e.target.value) })}
            />
          </label>
        )}
      </section>

      <section className="space-y-2">
        <Text variant={TextVariants.heading}>{t('editor.fujiRecipe.queue')}</Text>
        <div className="flex flex-wrap gap-2">
          <button
            className="px-2 py-1 rounded-md bg-accent text-bg-primary disabled:opacity-50"
            disabled={busy || !selectedImage || isCameraRender}
            onClick={handleEnqueue}
          >
            {t('editor.fujiRecipe.enqueue')}
          </button>
          <button
            className="px-2 py-1 rounded-md bg-bg-tertiary hover:bg-surface disabled:opacity-50"
            disabled={busy}
            onClick={handleProcessQueue}
          >
            <Play className="inline w-4 h-4 mr-1" />
            {t('editor.fujiRecipe.processQueue')}
          </button>
          <button
            className="px-2 py-1 rounded-md bg-bg-tertiary hover:bg-surface disabled:opacity-50"
            disabled={busy || renderStatus !== 'ready'}
            onClick={handleCreateVersion}
          >
            {t('editor.fujiRecipe.createVersion')}
          </button>
          <button
            className="px-2 py-1 rounded-md bg-bg-tertiary hover:bg-surface"
            onClick={handlePurgeCache}
          >
            <Trash2 className="inline w-4 h-4 mr-1" />
            {t('editor.fujiRecipe.purgeCache')}
          </button>
        </div>
        {queue.length === 0 ? (
          <p className="text-xs text-text-secondary">{t('editor.fujiRecipe.queueEmpty')}</p>
        ) : (
          <ul className="text-xs space-y-1">
            {queue.map((job) => (
              <li key={job.id} className="truncate">
                {statusLabel(job.status, t)} — {job.sourcePath.split('/').pop()}
                {job.error ? ` (${job.error})` : ''}
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
