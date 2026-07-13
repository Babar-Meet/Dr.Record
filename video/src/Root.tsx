import { Composition } from 'remotion';
import { MainVideo } from './Video';

export const RemotionRoot: React.FC = () => {
  return (
    <Composition
      id="DrRecord"
      component={MainVideo}
      durationInFrames={22 * 30}
      fps={30}
      width={1920}
      height={1080}
      defaultProps={{}}
    />
  );
};
