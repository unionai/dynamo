# SPDX-FileCopyrightText: Copyright (c) 2024-2025 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
# SPDX-License-Identifier: Apache-2.0
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
# http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

import logging
import sys

LOGGER_NAME = "Triton Worker"


class LogFormatter(logging.Formatter):
    """Class to handle formatting of the logger outputs"""

    def __init__(self, logger_name=LOGGER_NAME):
        logger = logging.getLogger(logger_name)
        self._log_level = logger.getEffectiveLevel()
        self._logger_name = logger_name
        super().__init__(datefmt="%H:%M:%S")

    def format(self, record):
        front = "%(asctime)s %(filename)s:%(lineno)s"
        self._style._fmt = f"{front}[{self._logger_name}] %(levelname)s: %(message)s"
        return super().format(record)


def setup_logger(log_level=1, logger_name=LOGGER_NAME):
    if log_level == 0:
        log_level = logging.ERROR
    elif log_level == 1:
        log_level = logging.INFO
    else:
        log_level = logging.DEBUG

    logger = logging.getLogger(logger_name)
    logger.setLevel(level=log_level)

    handler = logging.StreamHandler(sys.stdout)
    handler.setFormatter(LogFormatter(logger_name=logger_name))
    logger.addHandler(handler)
    logger.propagate = True

    return logger
