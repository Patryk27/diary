# diary

Application for managing my digital diary (consisting of photos, videos and
*.org notes).

Currently the main functionality is categorizing content, i.e. it transforms a
flat list of files:

``` 
IMG_1234.jpg
IMG_1235.jpg
IMG_1236.jpg
2018-01-01.org
2018-01-02.org
2018-01-03.org
```

... into a per-day tree:

```
2018/01/01/index.org
2018/01/01/12-34-56.jpg

2018/01/02/index.org
2018/01/02/21-37-00.jpg

2018/01/03/index.org
2018/01/03/16-35-00.jpg
```

... renaming files and passing videos through ffmpeg to make them lightweight.

Note that this is not meant to be any sort of _general-purpose_ diary manager.

## License

Copyright (c) 2024, Patryk Wychowaniec <pwychowaniec@pm.me>.    
Licensed under the MIT license.
