import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { toast } from 'react-toastify';
import { Camera, RefreshCw, Trash2, Play, Power } from 'lucide-react';
import { useEditorStore } from '../../../store/useEditorStore';
import { useEditorActions } from '../../../hooks/useEditorActions';
import { Invokes } from '../../ui/AppProperties';
import Text from '../../ui/Text';
import { TextVariants } from '../../../types/typography';
import Dropdown from '../../ui/Dropdown';
import FujiFilmIcon from '../../icons/FujiFilmIcon';

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

type ConvertQuality = 'full' | 'preview';

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

const GRAIN_OPTIONS = [
  { value: 'off', label: 'Off' },
  { value: 'weakSmall', label: 'Weak / Small' },
  { value: 'strongSmall', label: 'Strong / Small' },
  { value: 'weakLarge', label: 'Weak / Large' },
  { value: 'strongLarge', label: 'Strong / Large' },
] as const;

const EFFECT_OPTIONS = [
  { value: 'off', label: 'Off' },
  { value: 'weak', label: 'Weak' },
  { value: 'strong', label: 'Strong' },
] as const;

const WB_OPTIONS = [
  { value: 0x0000, label: 'As Shot' },
  { value: 0x0002, label: 'Auto' },
  { value: 0x0004, label: 'Daylight' },
  { value: 0x8006, label: 'Shade' },
  { value: 0x0006, label: 'Incandescent' },
  { value: 0x8007, label: 'Color Temperature' },
];

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

function ToneSlider({
  label,
  value,
  min,
  max,
  step,
  disabled,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  disabled: boolean;
  onChange: (v: number) => void;
}) {
  return (
    <label className="block">
      <span className="text-xs text-text-secondary">
        {label}: {value.toFixed(step < 1 ? 1 : 0)}
      </span>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        className="w-full"
        disabled={disabled}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
      />
    </label>
  );
}

export default function FujiRecipePanel() {
  const { t } = useTranslation();
  const selectedImage = useEditorStore((s) => s.selectedImage);
  const adjustments = useEditorStore((s) => s.adjustments);
  const { setAdjustments } = useEditorActions();

  const [support, setSupport] = useState<SupportInfo | null>(null);
  const [cameras, setCameras] = useState<DiscoveredCamera[]>([]);
  const [connected, setConnected] = useState<DiscoveredCamera | null>(null);
  const [queue, setQueue] = useState<QueueJob[]>([]);
  const [busy, setBusy] = useState(false);
  const [quality, setQuality] = useState<ConvertQuality>('full');
  const [showGuidance, setShowGuidance] = useState(false);

  const isCameraRender = Boolean(adjustments?.fujiCameraRender);
  const renderStatus = (adjustments?.fujiRenderStatus as string) || 'none';

  const recipe: FujiRecipe = useMemo(() => {
    return { ...DEFAULT_RECIPE, ...(adjustments?.fujiRecipe || {}) };
  }, [adjustments?.fujiRecipe]);

  const isMono = useMemo(
    () => FILM_SIMS.find((s) => s.value === recipe.filmSimulation)?.mono === true,
    [recipe.filmSimulation],
  );

  const filmOptions = useMemo(() => FILM_SIMS.map((s) => ({ value: s.value, label: s.label })), []);
  const drOptions = useMemo(
    () => [
      { value: 100, label: 'DR100' },
      { value: 200, label: 'DR200' },
      { value: 400, label: 'DR400' },
    ],
    [],
  );
  const qualityOptions = useMemo(
    () => [
      { value: 'full' as const, label: t('editor.fujiRecipe.qualityFull') },
      { value: 'preview' as const, label: t('editor.fujiRecipe.qualityPreview') },
    ],
    [t],
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

  const refreshConnection = useCallback(async () => {
    try {
      const status = await invoke<{ connected: boolean; busId?: string; modelName?: string }>(
        Invokes.FujiConnectionStatus,
      );
      if (status.connected && status.modelName) {
        setConnected({
          busId: status.busId || '',
          modelName: status.modelName,
          verified: true,
        });
      } else if (!status.connected) {
        setConnected(null);
      }
    } catch {
      /* ignore */
    }
  }, []);

  useEffect(() => {
    refreshSupport();
    refreshQueue();
    refreshConnection();
  }, [refreshSupport, refreshQueue, refreshConnection]);

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
      if (list.length === 0) {
        toast.info(t('editor.fujiRecipe.noCameras'));
        setShowGuidance(true);
      }
    } catch (e: any) {
      toast.error(String(e));
      setShowGuidance(true);
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
      setShowGuidance(true);
    } finally {
      setBusy(false);
    }
  };

  const handleDisconnect = async () => {
    setBusy(true);
    try {
      await invoke(Invokes.FujiDisconnect);
      setConnected(null);
      toast.info(t('editor.fujiRecipe.disconnected'));
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
      await refreshConnection();
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
        { quality },
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
        <div className="flex items-center gap-2">
          <FujiFilmIcon size={18} className="text-[#FB0020] shrink-0" />
          <Text variant={TextVariants.heading}>{t('editor.fujiRecipe.title')}</Text>
        </div>
        <p>{t('editor.fujiRecipe.notInBuild')}</p>
        <p className="text-xs">{support.platform.title}</p>
      </div>
    );
  }

  return (
    <div className="p-3 space-y-4 text-sm overflow-y-auto h-full">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2 min-w-0">
          <FujiFilmIcon size={18} className="text-[#FB0020] shrink-0" />
          <Text variant={TextVariants.heading}>{t('editor.fujiRecipe.title')}</Text>
        </div>
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
            className="px-2 py-1 rounded-md bg-surface hover:bg-card-active disabled:opacity-50"
            disabled={busy}
            onClick={handleListCameras}
          >
            <Camera className="inline w-4 h-4 mr-1" />
            {t('editor.fujiRecipe.scan')}
          </button>
          <button
            className="px-2 py-1 rounded-md bg-surface hover:bg-card-active disabled:opacity-50"
            disabled={busy || !connected}
            onClick={handleRecover}
          >
            <RefreshCw className="inline w-4 h-4 mr-1" />
            {t('editor.fujiRecipe.recover')}
          </button>
          {connected && (
            <button
              className="px-2 py-1 rounded-md bg-surface hover:bg-card-active disabled:opacity-50"
              disabled={busy}
              onClick={handleDisconnect}
            >
              <Power className="inline w-4 h-4 mr-1" />
              {t('editor.fujiRecipe.disconnect')}
            </button>
          )}
        </div>
        {cameras.map((cam) => (
          <button
            key={cam.busId}
            className="block w-full text-left px-2 py-1.5 rounded-md bg-surface hover:bg-card-active"
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
        <button
          className="text-xs text-text-secondary underline"
          onClick={() => setShowGuidance((v) => !v)}
        >
          {t('editor.fujiRecipe.usbGuidance')}
        </button>
        {showGuidance && support?.platform && (
          <div className="text-xs text-text-secondary space-y-1 border border-surface rounded-md p-2">
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
        <div>
          <span className="text-xs text-text-secondary">{t('editor.fujiRecipe.filmSim')}</span>
          <Dropdown
            className="mt-1"
            value={recipe.filmSimulation}
            options={filmOptions}
            disabled={isCameraRender}
            onChange={(v) => updateRecipe({ filmSimulation: Number(v) })}
          />
        </div>
        <div>
          <span className="text-xs text-text-secondary">{t('editor.fujiRecipe.dynamicRange')}</span>
          <Dropdown
            className="mt-1"
            value={recipe.dynamicRange}
            options={drOptions}
            disabled={isCameraRender}
            onChange={(v) => updateRecipe({ dynamicRange: Number(v) })}
          />
        </div>

        <ToneSlider
          label={t('editor.fujiRecipe.highlightTone')}
          value={recipe.highlightTone}
          min={-4}
          max={4}
          step={0.5}
          disabled={isCameraRender}
          onChange={(v) => updateRecipe({ highlightTone: v })}
        />
        <ToneSlider
          label={t('editor.fujiRecipe.shadowTone')}
          value={recipe.shadowTone}
          min={-4}
          max={4}
          step={0.5}
          disabled={isCameraRender}
          onChange={(v) => updateRecipe({ shadowTone: v })}
        />
        <ToneSlider
          label={t('editor.fujiRecipe.sharpness')}
          value={recipe.sharpness}
          min={-4}
          max={4}
          step={0.5}
          disabled={isCameraRender}
          onChange={(v) => updateRecipe({ sharpness: v })}
        />
        <ToneSlider
          label={t('editor.fujiRecipe.clarity')}
          value={recipe.clarity}
          min={-5}
          max={5}
          step={1}
          disabled={isCameraRender}
          onChange={(v) => updateRecipe({ clarity: v })}
        />
        {!isMono && (
          <ToneSlider
            label={t('editor.fujiRecipe.color')}
            value={recipe.color}
            min={-4}
            max={4}
            step={0.5}
            disabled={isCameraRender}
            onChange={(v) => updateRecipe({ color: v })}
          />
        )}
        {isMono && (
          <>
            <ToneSlider
              label={t('editor.fujiRecipe.monoWc')}
              value={recipe.monoWc}
              min={-9}
              max={9}
              step={1}
              disabled={isCameraRender}
              onChange={(v) => updateRecipe({ monoWc: v })}
            />
            <ToneSlider
              label={t('editor.fujiRecipe.monoMg')}
              value={recipe.monoMg}
              min={-9}
              max={9}
              step={1}
              disabled={isCameraRender}
              onChange={(v) => updateRecipe({ monoMg: v })}
            />
          </>
        )}

        <div>
          <span className="text-xs text-text-secondary">{t('editor.fujiRecipe.grain')}</span>
          <Dropdown
            className="mt-1"
            value={recipe.grain}
            options={[...GRAIN_OPTIONS]}
            disabled={isCameraRender}
            onChange={(v) => updateRecipe({ grain: v as FujiRecipe['grain'] })}
          />
        </div>
        <div>
          <span className="text-xs text-text-secondary">{t('editor.fujiRecipe.colorChrome')}</span>
          <Dropdown
            className="mt-1"
            value={recipe.colorChrome}
            options={[...EFFECT_OPTIONS]}
            disabled={isCameraRender}
            onChange={(v) => updateRecipe({ colorChrome: v as FujiRecipe['colorChrome'] })}
          />
        </div>
        <div>
          <span className="text-xs text-text-secondary">{t('editor.fujiRecipe.wb')}</span>
          <Dropdown
            className="mt-1"
            value={recipe.whiteBalance}
            options={WB_OPTIONS}
            disabled={isCameraRender}
            onChange={(v) => updateRecipe({ whiteBalance: Number(v) })}
          />
        </div>
        {recipe.whiteBalance === 0x8007 && (
          <ToneSlider
            label={t('editor.fujiRecipe.colorTemp')}
            value={recipe.colorTempK}
            min={2500}
            max={10000}
            step={10}
            disabled={isCameraRender}
            onChange={(v) => updateRecipe({ colorTempK: v })}
          />
        )}
      </section>

      <section className="space-y-2">
        <Text variant={TextVariants.heading}>{t('editor.fujiRecipe.queue')}</Text>
        <div>
          <span className="text-xs text-text-secondary">{t('editor.fujiRecipe.quality')}</span>
          <Dropdown
            className="mt-1"
            value={quality}
            options={qualityOptions}
            onChange={(v) => setQuality(v as ConvertQuality)}
          />
        </div>
        <div className="flex flex-wrap gap-2">
          <button
            className="px-2 py-1 rounded-md bg-accent text-button-text disabled:opacity-50"
            disabled={busy || !selectedImage || isCameraRender}
            onClick={handleEnqueue}
          >
            {t('editor.fujiRecipe.enqueue')}
          </button>
          <button
            className="px-2 py-1 rounded-md bg-surface hover:bg-card-active disabled:opacity-50"
            disabled={busy}
            onClick={handleProcessQueue}
          >
            <Play className="inline w-4 h-4 mr-1" />
            {t('editor.fujiRecipe.processQueue')}
          </button>
          <button
            className="px-2 py-1 rounded-md bg-surface hover:bg-card-active disabled:opacity-50"
            disabled={busy || renderStatus !== 'ready'}
            onClick={handleCreateVersion}
          >
            {t('editor.fujiRecipe.createVersion')}
          </button>
          <button
            className="px-2 py-1 rounded-md bg-surface hover:bg-card-active"
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
