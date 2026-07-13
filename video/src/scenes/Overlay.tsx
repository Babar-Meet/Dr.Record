import {
  AbsoluteFill,
  interpolate,
  spring,
  useCurrentFrame,
  useVideoConfig,
} from 'remotion';
import { colors, fonts } from '../theme';

export const Overlay: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  const slideIn = spring({
    frame,
    fps,
    config: { damping: 16, mass: 0.4 },
  });

  const elapsed = Math.floor(frame / fps);
  const minutes = String(Math.floor(elapsed / 60)).padStart(2, '0');
  const seconds = String(elapsed % 60).padStart(2, '0');

  const pulse = interpolate(Math.sin(frame * 0.15), [-1, 1], [1, 0.4]);

  return (
    <AbsoluteFill
      style={{
        justifyContent: 'center',
        alignItems: 'center',
        background: `radial-gradient(ellipse at center, ${colors.bg}, #000)`,
      }}
    >
      {/* Overlay pill */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 10,
          padding: '14px 28px',
          background: 'rgba(0,0,0,0.8)',
          border: '1px solid rgba(255,255,255,0.08)',
          borderRadius: 40,
          backdropFilter: 'blur(12px)',
          transform: `translateY(${(1 - slideIn) * 80}px) scale(${slideIn})`,
          opacity: slideIn,
        }}
      >
        <div
          style={{
            width: 12,
            height: 12,
            borderRadius: '50%',
            background: colors.red,
            opacity: pulse,
          }}
        />
        <span
          style={{
            color: colors.red,
            fontFamily: fonts.ui,
            fontSize: 18,
            fontWeight: 600,
            letterSpacing: '0.5px',
          }}
        >
          REC
        </span>
        <span
          style={{
            color: colors.textBright,
            fontFamily: fonts.mono,
            fontSize: 20,
            minWidth: 60,
            textAlign: 'center',
          }}
        >
          {minutes}:{seconds}
        </span>
      </div>

      {/* Caption */}
      <div
        style={{
          position: 'absolute',
          bottom: 120,
          textAlign: 'center',
          opacity: spring({
            frame: frame - 30,
            fps,
            config: { damping: 12, mass: 0.5 },
          }),
        }}
      >
        <p
          style={{
            fontFamily: fonts.ui,
            fontSize: 24,
            color: colors.textDim,
            margin: 0,
          }}
        >
          Minimal. Unobtrusive.
        </p>
        <p
          style={{
            fontFamily: fonts.ui,
            fontSize: 16,
            color: colors.textDim,
            margin: '8px 0 0 0',
            opacity: 0.6,
          }}
        >
          Just you and your content
        </p>
      </div>
    </AbsoluteFill>
  );
};
